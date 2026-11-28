use super::{
    ChangeSet, ResourceChange, ResourceChangeKind, ResourceWrite, WriteMode, revision::Expectations,
};
use crate::{
    DocumentEntry, Error, Limits, Manifest, Resource, Result, error::check_limit,
    resource::walk_hard_dependencies,
};
use std::collections::{BTreeSet, HashMap, HashSet};

type ResourceMap<'a> = HashMap<&'a str, Resource<'a>>;

pub(crate) struct ManifestPlan<'a, 'b> {
    pub previous: ResourceMap<'a>,
    pub proposed: ResourceMap<'b>,
    pub metadata: Vec<ResourceChange>,
}

pub(crate) fn check_limits<R>(changes: &ChangeSet<R>, limits: &Limits) -> Result<()> {
    for (kind, count) in [
        ("transaction expected resources", changes.expected.len()),
        ("transaction writes", changes.writes.len()),
        ("transaction removals", changes.removals.len()),
    ] {
        check_limit(kind, count as u64, limits.max_resources as u64)?;
    }
    Ok(())
}

pub(crate) fn check_identity(previous: &Manifest, next: &Manifest) -> Result<()> {
    if previous.project_id != next.project_id || !preserves(&previous.extensions, &next.extensions)
    {
        return Err(Error::Validation(
            "project identity or extension data would change".into(),
        ));
    }
    Ok(())
}

pub(crate) fn in_place<'a, R>(
    changes: &ChangeSet<R>,
    get: impl Fn(&str) -> Option<Resource<'a>>,
) -> Result<Option<&'a DocumentEntry>> {
    if !changes.removals.is_empty() || changes.writes.len() > 1 {
        return Err(Error::Validation(
            "multiple writes or removals require a new manifest and immutable paths".into(),
        ));
    }
    let Some(write) = changes.writes.first() else {
        return Ok(None);
    };
    let Some(Resource::Document(entry)) = get(&write.id) else {
        return Err(Error::Validation(
            "in-place replacement requires a registered document".into(),
        ));
    };
    if write.mode != WriteMode::Replace || write.path != entry.path {
        return Err(Error::Validation(
            "in-place replacement must target its current document path".into(),
        ));
    }
    if entry.links.iter().any(|link| link.relationship.is_hard()) {
        expect_dependencies(&get, &changes.expected, [entry.id.as_str()], &get)?;
    } else {
        expect_resource(
            &changes.expected,
            &entry.id,
            Some(Resource::Document(entry)),
        )?;
    }
    Ok(Some(entry))
}

pub(crate) fn manifest<'a, 'b, R>(
    previous_manifest: &'a Manifest,
    next: &'b Manifest,
    expected: &Expectations,
    removals: &BTreeSet<String>,
    writes: &[ResourceWrite<R>],
) -> Result<ManifestPlan<'a, 'b>> {
    let previous = resource_map(previous_manifest);
    let proposed = resource_map(next);
    let mut operations = HashSet::with_capacity(writes.len());
    for write in writes {
        let resource = proposed
            .get(write.id.as_str())
            .ok_or_else(|| Error::Missing(write.id.clone()))?;
        check_write(
            previous.get(write.id.as_str()).copied(),
            *resource,
            write.mode,
        )?;
        // Manifest validation already established globally unique paths.
        if resource.path() != Some(&write.path) || !operations.insert(write.id.as_str()) {
            return Err(Error::Validation(format!(
                "duplicate or mismatched write for {}",
                write.id
            )));
        }
    }
    let mut old_affected = Vec::new();
    let mut new_affected = Vec::new();
    if previous_manifest.links != next.links {
        let previous_hard = previous_manifest
            .links
            .iter()
            .filter(|link| link.relationship.is_hard())
            .collect::<HashSet<_>>();
        let next_hard = next
            .links
            .iter()
            .filter(|link| link.relationship.is_hard())
            .collect::<HashSet<_>>();
        old_affected.extend(
            previous_hard
                .difference(&next_hard)
                .map(|&link| link.target.as_str()),
        );
        new_affected.extend(
            next_hard
                .difference(&previous_hard)
                .map(|&link| link.target.as_str()),
        );
    }
    let mut metadata = Vec::new();
    for (&id, &old) in &previous {
        match proposed.get(id).copied() {
            None if removals.contains(id) => {
                old_affected.push(id);
                metadata.push(change(old, ResourceChangeKind::Removed, None));
            }
            None => {
                return Err(Error::Validation(format!(
                    "explicit removal required for {id}"
                )));
            }
            Some(_) if removals.contains(id) => {
                return Err(Error::Validation(format!("{id} is still registered")));
            }
            Some(new) => {
                check_extensions(old, new)?;
                if operations.contains(id) || !same_registration(old, new)? {
                    old_affected.push(id);
                    new_affected.push(new.id());
                    if !operations.contains(id) {
                        metadata.push(change(
                            new,
                            ResourceChangeKind::Metadata,
                            expected.get(id).cloned(),
                        ));
                    }
                }
            }
        }
    }
    for id in removals {
        if !previous.contains_key(id.as_str()) {
            return Err(Error::Missing(id.clone()));
        }
    }
    for (&id, &resource) in &proposed {
        if !previous.contains_key(id) {
            new_affected.push(id);
            if resource.path().is_none() {
                metadata.push(change(resource, ResourceChangeKind::Created, None));
            } else if !operations.contains(id) {
                return Err(Error::Validation(format!(
                    "staged creation required for {id}"
                )));
            }
        }
    }
    let before = |id: &str| previous.get(id).copied();
    let after = |id: &str| proposed.get(id).copied();
    expect_dependencies(before, expected, old_affected, before)?;
    expect_dependencies(before, expected, new_affected, after)?;
    Ok(ManifestPlan {
        previous,
        proposed,
        metadata,
    })
}

fn expect_dependencies<'a, 'b>(
    previous: impl Fn(&str) -> Option<Resource<'b>>,
    expected: &Expectations,
    ids: impl IntoIterator<Item = &'a str>,
    get: impl Fn(&str) -> Option<Resource<'a>>,
) -> Result<()> {
    walk_hard_dependencies(
        ids,
        |id| *id,
        |id| {
            get(id)
                .and_then(|resource| match resource {
                    Resource::Document(document) => Some(document.links.as_slice()),
                    _ => None,
                })
                .into_iter()
                .flatten()
                .map(|link| (link.relationship, link.target.as_str()))
        },
        |id| expect_resource(expected, id, previous(id)),
    )
}

fn expect_resource(
    expected: &Expectations,
    id: &str,
    previous: Option<Resource<'_>>,
) -> Result<()> {
    if previous.is_some_and(|resource| resource.path().is_some()) && !expected.contains_key(id) {
        return Err(Error::Validation(format!(
            "missing expected revision for {id}"
        )));
    }
    Ok(())
}

fn change(
    resource: Resource<'_>,
    kind: ResourceChangeKind,
    revision: Option<crate::Digest>,
) -> ResourceChange {
    ResourceChange {
        id: resource.id().into(),
        kind,
        revision,
    }
}

fn resource_map(manifest: &Manifest) -> ResourceMap<'_> {
    manifest
        .documents
        .iter()
        .map(Resource::Document)
        .chain(manifest.assets.iter().map(Resource::Asset))
        .chain(manifest.libraries.iter().map(Resource::ExternalLibrary))
        .map(|resource| (resource.id(), resource))
        .collect()
}

fn check_write(old: Option<Resource<'_>>, next: Resource<'_>, mode: WriteMode) -> Result<()> {
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
            next.id()
        ))),
    }
}

fn preserves(before: &crate::Extensions, after: &crate::Extensions) -> bool {
    before
        .iter()
        .all(|(name, value)| after.get(name) == Some(value))
}

fn check_extensions(old: Resource<'_>, new: Resource<'_>) -> Result<()> {
    let protected = match (old, new) {
        (Resource::Document(a), Resource::Document(b)) => preserves(&a.extensions, &b.extensions),
        (Resource::Asset(a), Resource::Asset(b)) => preserves(&a.extensions, &b.extensions),
        (Resource::ExternalLibrary(_), Resource::ExternalLibrary(_)) => true,
        _ => false,
    };
    if !protected {
        return Err(Error::Validation(format!(
            "registry extension data would be lost for {}",
            old.id()
        )));
    }
    Ok(())
}

fn same_registration(old: Resource<'_>, new: Resource<'_>) -> Result<bool> {
    match (old, new) {
        (Resource::Document(old), Resource::Document(new))
            if old.path == new.path
                && old.kind == new.kind
                && old.schema_version == new.schema_version =>
        {
            Ok(old == new)
        }
        (Resource::Asset(old), Resource::Asset(new)) if old.path == new.path => Ok(old == new),
        (Resource::ExternalLibrary(old), Resource::ExternalLibrary(new)) => Ok(old == new),
        _ => Err(Error::Validation(format!(
            "resource {} needs an explicit write",
            new.id()
        ))),
    }
}
