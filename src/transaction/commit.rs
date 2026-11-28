use super::{
    ChangeSet, ResourceChange, ResourceChangeKind, TransactionOutcome, WriteMode, planner,
    recovery::{self, ANCHOR_STAGE, Journal, JournalFile},
    revision::{verify_expected, verify_expected_excluding, verify_revision},
};
use crate::storage::{
    Project, ensure_manifest_path, no_link_metadata, read_bounded, read_file, resolve, write_lock,
};
use crate::{
    Digest, DocumentEntry, Error, Manifest, Resource, Result, decode_document, error::io,
    integrity::transfer, validate_document_transition,
};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::Path,
};

pub(crate) struct Publication {
    pub manifest: Option<(Manifest, Digest)>,
    pub outcome: TransactionOutcome,
}

type Validate<'a> =
    dyn FnMut(WriteMode, &DocumentEntry, &[u8], &crate::DocumentRead<'_>) -> Result<()> + 'a;

pub(crate) fn execute<R: Read>(
    project: &Project,
    mut changes: ChangeSet<R>,
    registry: &crate::CodecRegistry,
    mut validate: impl FnMut(WriteMode, &DocumentEntry, &[u8], &crate::DocumentRead<'_>) -> Result<()>,
) -> Result<Publication> {
    planner::check_limits(&changes, project.limits())?;
    if &changes.revision != project.revision() {
        return Err(Error::Conflict("manifest revision".into()));
    }

    let _lock = write_lock(project.root())?;
    project.ensure_no_pending()?;
    project.check_revision()?;
    if let Some(next) = changes.manifest.take() {
        let bytes = next.to_bytes(project.limits())?;
        ensure_manifest_path(&next, project.manifest_path())?;
        planner::check_identity(project.manifest(), &next)?;
        let digest = Digest::of(&bytes);
        if &digest != project.revision() {
            return manifest_change(
                project,
                changes,
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

fn in_place<R: Read>(
    project: &Project,
    mut changes: ChangeSet<R>,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<Publication> {
    let Some(entry) = planner::in_place(&changes, |id| project.resource(id))? else {
        verify_expected(project, &changes.expected, None)?;
        return Ok(Publication {
            manifest: None,
            outcome: TransactionOutcome::new(
                project.revision().clone(),
                false,
                vec![],
                None,
                false,
            ),
        });
    };
    let write = changes.writes.pop().expect("in-place write selected");
    let before = &changes.expected[&entry.id];
    let destination = resolve(project.root(), &entry.path, false)?;
    let previous = read_file(project.root(), &entry.path, project.limits().max_json_bytes)?;
    verify_revision(Digest::of(&previous), before, &entry.id)?;
    verify_expected(project, &changes.expected, Some(&entry.id))?;
    let current = read_bounded(
        write.reader,
        project
            .limits()
            .max_json_bytes
            .min(project.limits().max_total_bytes),
        entry.path.as_str(),
    )?;
    let digest = Digest::of(&current);
    let published = &digest != before;
    let completion = if !published {
        verify_expected(project, &changes.expected, None)?;
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
        verify_expected(project, &changes.expected, None)?;
        project.check_revision()?;
        crate::storage::replace_temp(staging, &destination)?
    };
    let resources = if published {
        vec![ResourceChange {
            id: entry.id.clone(),
            kind: ResourceChangeKind::Replaced,
            revision: Some(digest),
        }]
    } else {
        vec![]
    };
    Ok(Publication {
        manifest: None,
        outcome: TransactionOutcome::new(
            project.revision().clone(),
            published,
            resources,
            completion.err(),
            false,
        ),
    })
}

fn manifest_change<R: Read>(
    project: &Project,
    changes: ChangeSet<R>,
    next: Manifest,
    manifest_bytes: Vec<u8>,
    digest: Digest,
    registry: &crate::CodecRegistry,
    validate: &mut Validate<'_>,
) -> Result<Publication> {
    let ChangeSet {
        expected,
        removals,
        writes,
        ..
    } = changes;
    let planner::ManifestPlan {
        previous,
        proposed,
        mut metadata,
    } = planner::manifest(project.manifest(), &next, &expected, &removals, &writes)?;
    // Authored replacements validate the exact source bytes immediately before
    // staging that resource. Hashing it separately here would repeat that read.
    let authored = writes
        .iter()
        .filter(|write| {
            matches!(write.mode, WriteMode::Replace | WriteMode::Migrate)
                && matches!(previous.get(write.id.as_str()), Some(Resource::Document(_)))
        })
        .map(|write| write.id.as_str())
        .collect::<HashSet<_>>();
    verify_expected_excluding(project, &expected, |id| authored.contains(id))?;

    let stage = recovery::begin(project.root())?;
    let result = (|| {
        let mut list = Vec::with_capacity(writes.len());
        let mut total = 0;
        for write in writes {
            let resource = proposed[write.id.as_str()];
            let old_document = match (write.mode, previous.get(write.id.as_str())) {
                (WriteMode::Replace | WriteMode::Migrate, Some(Resource::Document(old))) => {
                    let bytes =
                        read_file(project.root(), &old.path, project.limits().max_json_bytes)?;
                    verify_revision(Digest::of(&bytes), &expected[&write.id], &old.id)?;
                    Some((*old, bytes))
                }
                _ => None,
            };
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
                let current = read_staged(&file, project.limits().max_json_bytes)?;
                let previous = old_document
                    .as_ref()
                    .map(|(old, bytes)| (*old, bytes.as_slice()));
                validate_document(
                    project, entry, &current, previous, write.mode, registry, validate,
                )?;
            }
            metadata.push(ResourceChange {
                id: write.id,
                kind: match write.mode {
                    WriteMode::Create => ResourceChangeKind::Created,
                    WriteMode::Replace => ResourceChangeKind::Replaced,
                    WriteMode::Relocate => ResourceChangeKind::Relocated,
                    WriteMode::Migrate => ResourceChangeKind::Migrated,
                },
                revision: Some(sha256.clone()),
            });
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
        verify_expected(project, &expected, None)?;
        for (index, file) in journal.files.iter().enumerate() {
            let destination = resolve(project.root(), &file.path, true)?;
            if no_link_metadata(&destination)?.is_some() {
                return Err(Error::Conflict(file.path.to_string()));
            }
            let source = stage.join(recovery::data_name(index));
            crate::storage::publish_new_resource(&source, &destination, file.bytes)?;
        }
        verify_expected(project, &expected, None)?;
        project.check_revision()?;
        crate::storage::replace_staged(
            &anchor_stage,
            &project.root().join(project.manifest_path().as_path()),
        )
    })();
    let mut completion = match result {
        Ok(completion) => completion,
        Err(error) => {
            // Retain evidence after the durable journal has been installed.
            if matches!(
                recovery::inspect(project.root(), project.manifest_path(), project.limits()),
                Ok(super::RecoveryState::Staging)
            ) {
                let _ = recovery::cleanup(project.root(), project.limits().max_resources);
            }
            return Err(error);
        }
    };
    if completion.is_ok()
        && let Err(source) = recovery::cleanup(project.root(), project.limits().max_resources)
    {
        completion = Err(Error::PublishedCleanup(Box::new(source)));
    }
    let recovery_required = completion.is_err() && project.ensure_no_pending().is_err();
    Ok(Publication {
        manifest: Some((next, digest.clone())),
        outcome: TransactionOutcome::new(
            digest,
            true,
            metadata,
            completion.err(),
            recovery_required,
        ),
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
    let (count, digest) = transfer(input, &mut file, limit.min(remaining), resource.id())?;
    file.sync_all()
        .map_err(|error| io(path.display().to_string(), error))?;
    Ok((digest, count))
}

fn read_staged(file: &Path, limit: u64) -> Result<Vec<u8>> {
    let input = fs::File::open(file).map_err(|error| io(file.display().to_string(), error))?;
    read_bounded(input, limit, "staged document")
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
        if mode == WriteMode::Migrate {
            registry.validate_decoded_transition(
                (old, old_bytes, &previous.document),
                (entry, current, &decoded),
            )?;
        } else {
            validate_document_transition(entry, &previous.document, &decoded)?;
        }
    }
    validate(mode, entry, current, &decoded)
}
