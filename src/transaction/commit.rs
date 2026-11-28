use super::{
    ChangeSet, ResourceWrite, WriteMode,
    recovery::{self, ANCHOR_STAGE, Journal, JournalFile},
};
use crate::storage::{
    Project, ensure_manifest_path, no_link_metadata, read_bounded, read_file, resolve, write_lock,
};
use crate::{
    Digest, DocumentEntry, Error, Manifest, Relationship, Resource, Result, decode_document,
    error::io, integrity::transfer, validate_document_transition,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::Path,
};

pub(crate) struct Publication {
    pub manifest: Option<(Manifest, Digest)>,
    pub document: Option<Digest>,
    pub completion: Result<()>,
}

type Validate<'a> =
    dyn FnMut(WriteMode, &DocumentEntry, &[u8], &crate::DocumentRead<'_>) -> Result<()> + 'a;

pub(crate) fn execute<R: Read>(
    project: &Project,
    changes: ChangeSet<R>,
    registry: &crate::CodecRegistry,
    mut validate: impl FnMut(WriteMode, &DocumentEntry, &[u8], &crate::DocumentRead<'_>) -> Result<()>,
) -> Result<Publication> {
    let _lock = write_lock(project.root())?;
    project.ensure_no_pending()?;
    project.check_revision()?;
    if &changes.revision != project.revision() {
        return Err(Error::Conflict("manifest revision".into()));
    }
    if let Some(next) = changes.manifest.as_ref() {
        let bytes = next.to_bytes(project.limits())?;
        ensure_manifest_path(next, project.manifest_path())?;
        if next.project_id != project.manifest().project_id
            || !preserves(&project.manifest().extensions, &next.extensions)
        {
            return Err(Error::Validation(
                "project identity or extension data would change".into(),
            ));
        }
        let digest = Digest::of(&bytes);
        if &digest != project.revision() {
            let next = changes.manifest.expect("checked manifest");
            return manifest_change(
                project,
                changes.expected,
                changes.removals,
                changes.writes,
                next,
                bytes,
                digest,
                registry,
                &mut validate,
            );
        }
    }
    in_place(project, changes, registry, &mut validate)
}

fn preserves(before: &crate::Extensions, after: &crate::Extensions) -> bool {
    before
        .iter()
        .all(|(name, value)| after.get(name) == Some(value))
}

fn expect_resource(
    project: &Project,
    expected: &std::collections::BTreeMap<String, Digest>,
    id: &str,
) -> Result<()> {
    if project
        .resource(id)
        .is_some_and(|resource| resource.path().is_some())
        && !expected.contains_key(id)
    {
        return Err(Error::Validation(format!(
            "missing expected revision for {id}"
        )));
    }
    Ok(())
}

fn verify_expected(
    project: &Project,
    expected: &std::collections::BTreeMap<String, Digest>,
) -> Result<()> {
    verify_expected_except(project, expected, None)
}

fn verify_expected_except(
    project: &Project,
    expected: &std::collections::BTreeMap<String, Digest>,
    already_checked: Option<&str>,
) -> Result<()> {
    for (id, revision) in expected {
        if Some(id.as_str()) == already_checked {
            continue;
        }
        if &project.resource_revision(id)? != revision {
            return Err(Error::Conflict(id.clone()));
        }
    }
    Ok(())
}

fn expect_dependencies<'a>(
    project: &Project,
    expected: &std::collections::BTreeMap<String, Digest>,
    ids: impl IntoIterator<Item = &'a str>,
    get: impl Fn(&str) -> Option<Resource<'a>>,
) -> Result<()> {
    let mut pending: Vec<_> = ids.into_iter().collect();
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        expect_resource(project, expected, id)?;
        if let Some(Resource::Document(document)) = get(id) {
            pending.extend(
                document
                    .links
                    .iter()
                    .filter(|link| {
                        matches!(
                            link.relationship,
                            Relationship::Dependency | Relationship::Library
                        )
                    })
                    .map(|link| link.target.as_str()),
            );
        }
    }
    Ok(())
}

fn in_place<R: Read>(
    project: &Project,
    mut changes: ChangeSet<R>,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<Publication> {
    if !changes.removals.is_empty() || changes.writes.len() > 1 {
        return Err(Error::Validation(
            "multiple writes or removals require a new manifest and immutable paths".into(),
        ));
    }
    let Some(write) = changes.writes.pop() else {
        verify_expected(project, &changes.expected)?;
        return Ok(Publication {
            manifest: None,
            document: None,
            completion: Ok(()),
        });
    };
    let Some(Resource::Document(entry)) = project.resource(&write.id) else {
        return Err(Error::Validation(
            "in-place replacement requires a registered document".into(),
        ));
    };
    if write.mode != WriteMode::Replace || write.path != entry.path {
        return Err(Error::Validation(
            "in-place replacement must target its current document path".into(),
        ));
    }
    expect_dependencies(project, &changes.expected, [entry.id.as_str()], |id| {
        project.resource(id)
    })?;
    let before = &changes.expected[&entry.id];
    let destination = resolve(project.root(), &entry.path, false)?;
    let previous = read_file(project.root(), &entry.path, project.limits().max_json_bytes)?;
    if &Digest::of(&previous) != before {
        return Err(Error::Conflict(entry.id.clone()));
    }
    verify_expected_except(project, &changes.expected, Some(&entry.id))?;
    let current = read_bounded(
        write.reader,
        project
            .limits()
            .max_json_bytes
            .min(project.limits().max_total_bytes),
        entry.path.as_str(),
    )?;
    let digest = Digest::of(&current);
    let completion = if &digest == before {
        verify_expected(project, &changes.expected)?;
        project.check_revision()?;
        Ok(())
    } else {
        validate_document(
            project,
            entry,
            &current,
            Some((entry, &previous)),
            WriteMode::Replace,
            registry,
            validate,
        )?;
        let staging = crate::storage::temp_bytes(
            destination.parent().expect("project resource has parent"),
            &current,
        )?;
        verify_expected(project, &changes.expected)?;
        project.check_revision()?;
        crate::storage::replace_temp(staging, &destination)?
    };
    Ok(Publication {
        manifest: None,
        document: Some(digest),
        completion,
    })
}

fn resource_id<'a>(value: &Resource<'a>) -> &'a str {
    match value {
        Resource::Document(doc) => &doc.id,
        Resource::Asset(asset) => &asset.id,
        Resource::ExternalLibrary(library) => &library.id,
    }
}

fn resource_map(manifest: &Manifest) -> HashMap<&str, Resource<'_>> {
    manifest
        .documents
        .iter()
        .map(Resource::Document)
        .chain(manifest.assets.iter().map(Resource::Asset))
        .chain(manifest.libraries.iter().map(Resource::ExternalLibrary))
        .map(|resource| (resource_id(&resource), resource))
        .collect()
}

fn plan_change(old: Option<Resource<'_>>, next: Resource<'_>, mode: WriteMode) -> Result<()> {
    match (old, next, mode) {
        (None, Resource::Document(_) | Resource::Asset(_), WriteMode::Create) => Ok(()),
        (
            Some(Resource::Asset(old)),
            Resource::Asset(new),
            WriteMode::Replace | WriteMode::Relocate,
        ) if old.path != new.path => Ok(()),
        (Some(Resource::Document(old)), Resource::Document(new), mode)
            if old.path != new.path && old.kind == new.kind =>
        {
            match mode {
                WriteMode::Replace | WriteMode::Relocate
                    if old.schema_version == new.schema_version =>
                {
                    Ok(())
                }
                WriteMode::Migrate if old.schema_version != new.schema_version => Ok(()),
                _ => Err(Error::Validation(format!(
                    "invalid document transition for {}",
                    old.id
                ))),
            }
        }
        _ => Err(Error::Validation(format!(
            "invalid change plan for {}",
            resource_id(&next)
        ))),
    }
}

fn same_registration(old: Resource<'_>, new: Resource<'_>) -> Result<bool> {
    match (old, new) {
        (Resource::Document(old), Resource::Document(new))
            if preserves(&old.extensions, &new.extensions)
                && old.path == new.path
                && old.kind == new.kind
                && old.schema_version == new.schema_version =>
        {
            Ok(old == new)
        }
        (Resource::Asset(old), Resource::Asset(new))
            if preserves(&old.extensions, &new.extensions) && old.path == new.path =>
        {
            Ok(old == new)
        }
        (Resource::ExternalLibrary(old), Resource::ExternalLibrary(new)) => Ok(old == new),
        _ => Err(Error::Validation(format!(
            "resource {} needs an explicit write",
            resource_id(&new)
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
fn manifest_change<R: Read>(
    project: &Project,
    expected: std::collections::BTreeMap<String, Digest>,
    removals: std::collections::BTreeSet<String>,
    writes: Vec<ResourceWrite<R>>,
    next: Manifest,
    manifest_bytes: Vec<u8>,
    digest: Digest,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<Publication> {
    let previous = resource_map(project.manifest());
    let proposed = resource_map(&next);
    let mut operations = HashMap::new();
    let mut destinations = HashSet::new();
    for write in &writes {
        let resource = proposed
            .get(write.id.as_str())
            .ok_or_else(|| Error::Missing(write.id.clone()))?;
        plan_change(
            previous.get(write.id.as_str()).copied(),
            *resource,
            write.mode,
        )?;
        if resource.path() != Some(&write.path)
            || !destinations.insert(&write.path)
            || operations.insert(write.id.as_str(), write.mode).is_some()
        {
            return Err(Error::Validation(format!(
                "duplicate or mismatched write for {}",
                write.id
            )));
        }
    }
    let mut affected = Vec::new();
    for (&id, &old) in &previous {
        match proposed.get(id).copied() {
            None if removals.contains(id) => affected.push(id),
            None => {
                return Err(Error::Validation(format!(
                    "explicit removal required for {id}"
                )));
            }
            Some(_) if removals.contains(id) => {
                return Err(Error::Validation(format!("{id} is still registered")));
            }
            Some(new) if operations.contains_key(id) => {
                let protected = match (old, new) {
                    (Resource::Document(a), Resource::Document(b)) => {
                        preserves(&a.extensions, &b.extensions)
                    }
                    (Resource::Asset(a), Resource::Asset(b)) => {
                        preserves(&a.extensions, &b.extensions)
                    }
                    _ => false,
                };
                if !protected {
                    return Err(Error::Validation(format!(
                        "registry extension data would be lost for {id}"
                    )));
                }
                affected.push(id);
            }
            Some(new) => {
                if !same_registration(old, new)? {
                    affected.push(id);
                }
            }
        }
    }
    for id in &removals {
        if !previous.contains_key(id.as_str()) {
            return Err(Error::Missing(id.clone()));
        }
    }
    for (&id, resource) in &proposed {
        if !previous.contains_key(id) && resource.path().is_some() {
            if !operations.contains_key(id) {
                return Err(Error::Validation(format!(
                    "staged creation required for {id}"
                )));
            }
            affected.push(id);
        }
    }
    expect_dependencies(project, &expected, affected.iter().copied(), |id| {
        previous.get(id).copied()
    })?;
    expect_dependencies(project, &expected, affected, |id| proposed.get(id).copied())?;
    verify_expected(project, &expected)?;

    let stage = recovery::begin(project.root())?;
    let result = (|| {
        let mut list = Vec::new();
        let mut total = 0;
        for write in writes {
            let resource = proposed[write.id.as_str()];
            let file = stage.join(recovery::data_name(list.len()));
            let (sha256, size) = stage_resource(
                project,
                &file,
                write.reader,
                resource,
                project.limits().max_total_bytes - total,
            )?;
            total += size;
            if write.mode == WriteMode::Relocate {
                if expected.get(&write.id) != Some(&sha256) {
                    return Err(Error::Validation(format!(
                        "relocation changed the bytes of {}",
                        write.id
                    )));
                }
                if let Resource::Document(entry) = resource {
                    decode_document(
                        entry,
                        &read_staged(&file, project.limits().max_json_bytes)?,
                        project.limits(),
                    )?;
                }
            } else if let Resource::Document(entry) = resource {
                let old = match previous.get(write.id.as_str()) {
                    Some(Resource::Document(doc)) => Some(*doc),
                    _ => None,
                };
                check_document(project, old, entry, &file, write.mode, registry, validate)?;
            }
            list.push(JournalFile {
                path: write.path,
                digest: sha256,
                bytes: size,
            });
        }
        let anchor_stage = stage.join(ANCHOR_STAGE);
        let mut file = recovery::create_file(&anchor_stage)?;
        file.write_all(&manifest_bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| io("staged manifest", error))?;
        drop(file);
        let journal = Journal {
            version: 1,
            manifest: project.manifest_path().clone(),
            before: project.revision().clone(),
            after: digest.clone(),
            files: list,
        };
        recovery::write_journal(&stage, &journal)?;
        verify_expected(project, &expected)?;
        for (index, file) in journal.files.iter().enumerate() {
            let destination = resolve(project.root(), &file.path, true)?;
            if no_link_metadata(&destination)?.is_some() {
                return Err(Error::Conflict(file.path.to_string()));
            }
            let source = stage.join(recovery::data_name(index));
            crate::storage::publish_new_resource(&source, &destination, file.bytes)?;
        }
        verify_expected(project, &expected)?;
        project.check_revision()?;
        crate::storage::replace_staged(
            &anchor_stage,
            &project.root().join(project.manifest_path().as_path()),
        )
    })();
    finish(project, result, |completion| Publication {
        manifest: Some((next, digest)),
        document: None,
        completion,
    })
}

fn stage_resource(
    project: &Project,
    path: &Path,
    input: impl Read,
    resource: Resource<'_>,
    remaining: u64,
) -> Result<(Digest, u64)> {
    let limit = match resource {
        Resource::Document(_) => project.limits().max_json_bytes,
        _ => project.limits().max_asset_bytes,
    };
    let mut file = recovery::create_file(path)?;
    let (count, digest) = transfer(
        input,
        &mut file,
        limit.min(remaining),
        resource_id(&resource),
    )?;
    file.sync_all()
        .map_err(|error| io(path.display().to_string(), error))?;
    Ok((digest, count))
}

fn read_staged(file: &Path, limit: u64) -> Result<Vec<u8>> {
    let input = fs::File::open(file).map_err(|error| io(file.display().to_string(), error))?;
    read_bounded(input, limit, "staged document")
}

fn check_document(
    project: &Project,
    previous: Option<&DocumentEntry>,
    entry: &DocumentEntry,
    file: &Path,
    mode: WriteMode,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<()> {
    let current = read_staged(file, project.limits().max_json_bytes)?;
    let previous_bytes = previous
        .map(|old| read_file(project.root(), &old.path, project.limits().max_json_bytes))
        .transpose()?;
    let previous = previous.zip(previous_bytes.as_deref());
    validate_document(project, entry, &current, previous, mode, registry, validate)
}

fn validate_document(
    project: &Project,
    entry: &DocumentEntry,
    current: &[u8],
    previous: Option<(&DocumentEntry, &[u8])>,
    mode: WriteMode,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<()> {
    let decoded = registry.inspect(entry, current, project.limits())?.document;
    if matches!(decoded, crate::DocumentRead::Opaque(_)) {
        return Err(Error::Unsupported {
            domain: "document write",
            version: entry.schema_version.clone(),
        });
    }
    if let Some((old, old_bytes)) = previous {
        let previous = registry.inspect(old, old_bytes, project.limits())?;
        if mode == WriteMode::Migrate && !previous.capabilities.migratable {
            return Err(Error::Unsupported {
                domain: "source document migration",
                version: old.schema_version.clone(),
            });
        }
        let before = previous.document;
        validate_document_transition(entry, &before, &decoded)?;
    }
    validate(mode, entry, current, &decoded)
}

fn finish<T>(
    project: &Project,
    result: Result<T>,
    published: impl FnOnce(T) -> Publication,
) -> Result<Publication> {
    let result = match result {
        Ok(published_value) => published_value,
        Err(error) => {
            // Preserve evidence whenever a journal exists and a publication attempt failed.
            if matches!(
                recovery::inspect(project.root(), project.manifest_path(), project.limits()),
                Ok(super::RecoveryState::Staging)
            ) {
                let _ = recovery::cleanup(project.root(), project.limits().max_resources);
            }
            return Err(error);
        }
    };
    let mut publication = published(result);
    if publication.completion.is_ok()
        && let Err(source) = recovery::cleanup(project.root(), project.limits().max_resources)
    {
        publication.completion = Err(Error::PublishedCleanup(Box::new(source)));
    }
    Ok(publication)
}
