use crate::{Digest, Manifest, ProjectPath};
use std::collections::{BTreeMap, BTreeSet};

/// The intent determines validation and publication rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    Create,
    Replace,
    Relocate,
    Migrate,
}

/// A single incoming resource stream at the proposed manifest path.
pub struct ResourceWrite<R> {
    pub(crate) id: String,
    pub(crate) path: ProjectPath,
    pub(crate) mode: WriteMode,
    pub(crate) reader: R,
}

impl<R> ResourceWrite<R> {
    pub fn new(id: impl Into<String>, path: ProjectPath, mode: WriteMode, reader: R) -> Self {
        Self {
            id: id.into(),
            path,
            mode,
            reader,
        }
    }
}

/// Optimistic write plan. Capture resource tokens when reading data used by the plan.
pub struct ChangeSet<R> {
    pub(crate) revision: Digest,
    pub(crate) manifest: Option<Manifest>,
    pub(crate) expected: BTreeMap<String, Digest>,
    pub(crate) writes: Vec<ResourceWrite<R>>,
    pub(crate) removals: BTreeSet<String>,
}

impl<R> ChangeSet<R> {
    pub fn new(expected_manifest: Digest) -> Self {
        Self {
            revision: expected_manifest,
            manifest: None,
            expected: BTreeMap::new(),
            writes: vec![],
            removals: BTreeSet::new(),
        }
    }

    pub fn set_manifest(&mut self, manifest: Manifest) {
        self.manifest = Some(manifest);
    }

    /// Check a resource read from the previous project, including caller-specific dependencies.
    pub fn expect(&mut self, id: impl Into<String>, revision: Digest) {
        self.expected.insert(id.into(), revision);
    }

    pub fn write(&mut self, resource: ResourceWrite<R>) {
        self.writes.push(resource);
    }

    /// Unregister a resource. Its previously authored file remains available for explicit cleanup.
    pub fn remove(&mut self, id: impl Into<String>) {
        self.removals.insert(id.into());
    }
}
