use crate::{
    Digest, Error, Result,
    storage::{Project, Snapshot},
};
use std::collections::BTreeMap;

pub(crate) type Expectations = BTreeMap<String, Digest>;

/// A bounded group of requested local snapshots and byte revision tokens.
/// Asset tokens identify their exact bytes without buffering the assets.
#[derive(Debug)]
pub struct ValidatedReadSet {
    pub(crate) manifest: Digest,
    pub(crate) expected: Expectations,
    pub(crate) documents: BTreeMap<String, Snapshot>,
}

impl ValidatedReadSet {
    pub fn manifest_revision(&self) -> &Digest {
        &self.manifest
    }

    pub fn resource_revision(&self, id: &str) -> Option<&Digest> {
        self.expected.get(id)
    }

    /// Iterate captured content tokens in deterministic resource ID order.
    pub fn resource_revisions(&self) -> impl ExactSizeIterator<Item = (&str, &Digest)> {
        self.expected
            .iter()
            .map(|(id, revision)| (id.as_str(), revision))
    }

    /// A document snapshot is available only if the capture buffered that document.
    pub fn document(&self, id: &str) -> Option<&Snapshot> {
        self.documents.get(id)
    }
}

pub(crate) fn verify_expected(
    project: &Project,
    expected: &Expectations,
    already_checked: Option<&str>,
) -> Result<()> {
    verify_expected_excluding(project, expected, |id| Some(id) == already_checked)
}

pub(crate) fn verify_expected_excluding(
    project: &Project,
    expected: &Expectations,
    mut skip: impl FnMut(&str) -> bool,
) -> Result<()> {
    for (id, revision) in expected {
        if skip(id) {
            continue;
        }
        verify_revision(project.resource_revision(id)?, revision, id)?;
    }
    Ok(())
}

pub(crate) fn verify_revision(actual: Digest, expected: &Digest, id: &str) -> Result<()> {
    if &actual != expected {
        return Err(Error::Conflict(id.into()));
    }
    Ok(())
}
