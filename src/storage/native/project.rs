//! Local filesystem adapter. The manifest is the commit point for staged multi-file changes.

use super::{
    FilePublicationOutcome, open_regular, persist_new, read_file, temp_bytes, trusted_root,
    verify_file, write_lock,
};
use crate::{
    CodecRegistry, Digest, Error, Limits, Manifest, ProjectPath, Resource, Result, decode_document,
    decode_manifest,
    error::{check_limit, io},
    hash_reader,
    transaction::{
        self, ChangeSet, ResourceWrite, TransactionOutcome, TransactionRejection, ValidatedReadSet,
        WriteMode,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
/// An independently opened manifest snapshot, with an optimistic revision token.
pub struct Project {
    root: PathBuf,
    manifest_path: ProjectPath,
    manifest: Manifest,
    revision: Digest,
    limits: Limits,
    documents: HashMap<String, usize>,
    assets: HashMap<String, usize>,
    libraries: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub revision: Digest,
}

/// A manifest installed at its destination and the result of directory synchronization.
pub struct CreateProjectOutcome {
    pub project: Project,
    pub publication: FilePublicationOutcome,
}

/// New bytes for a registered immutable path in a staged commit.
pub struct StagedFile<R> {
    _path: ProjectPath,
    _reader: R,
}

impl<R: Read> StagedFile<R> {
    pub fn new(path: ProjectPath, reader: R) -> Self {
        Self {
            _path: path,
            _reader: reader,
        }
    }
}

impl Project {
    /// Open only the manifest and its index. Engineering files are opened on demand.
    pub fn open(path: impl AsRef<Path>, limits: Limits) -> Result<Self> {
        let (root, manifest_path) = manifest_location(path.as_ref())?;
        let bytes = read_file(&root, &manifest_path, limits.max_json_bytes)?;
        let revision = Digest::of(&bytes);
        let manifest = decode_manifest(&bytes, &limits)?;
        ensure_manifest_path(&manifest, &manifest_path)?;
        Ok(Self::from_parts(
            root,
            manifest_path,
            manifest,
            revision,
            limits,
        ))
    }

    /// Create a manifest without replacing an existing file.
    pub fn create(path: impl AsRef<Path>, manifest: Manifest, limits: Limits) -> Result<Self> {
        let outcome = Self::create_with_outcome(path, manifest, limits)?;
        outcome.publication.into_result()?;
        Ok(outcome.project)
    }

    /// Create without replacing an existing manifest. On success the manifest is
    /// installed, even if the publication outcome carries a durability warning.
    pub fn create_with_outcome(
        path: impl AsRef<Path>,
        manifest: Manifest,
        limits: Limits,
    ) -> Result<CreateProjectOutcome> {
        let bytes = manifest.to_bytes(&limits)?;
        let (root, manifest_path) = manifest_location(path.as_ref())?;
        ensure_manifest_path(&manifest, &manifest_path)?;
        let _lock = write_lock(&root)?;
        transaction::recovery::ensure_clean(&root)?;
        let project = Self::from_parts(root, manifest_path, manifest, Digest::of(&bytes), limits);
        project.validate_resources()?;
        let file = temp_bytes(&project.root, &bytes)?;
        let publication = persist_new(file, &project.root.join(project.manifest_path.as_path()))?;
        Ok(CreateProjectOutcome {
            project,
            publication,
        })
    }

    pub(crate) fn from_parts(
        root: PathBuf,
        manifest_path: ProjectPath,
        manifest: Manifest,
        revision: Digest,
        limits: Limits,
    ) -> Self {
        let documents = manifest
            .documents
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.clone(), index))
            .collect();
        let assets = manifest
            .assets
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.clone(), index))
            .collect();
        let libraries = manifest
            .libraries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.clone(), index))
            .collect();
        Self {
            root,
            manifest_path,
            manifest,
            revision,
            limits,
            documents,
            assets,
            libraries,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn revision(&self) -> &Digest {
        &self.revision
    }
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Read a bounded, validated document. Use decode_document to inspect its capability.
    pub fn read_document(&self, id: &str) -> Result<Snapshot> {
        self.read_document_bounded(self.document(id)?, self.limits.max_json_bytes)
    }

    fn read_document_bounded(&self, entry: &crate::DocumentEntry, limit: u64) -> Result<Snapshot> {
        let bytes = read_file(
            &self.root,
            &entry.path,
            limit.min(self.limits.max_json_bytes),
        )?;
        decode_document(entry, &bytes, &self.limits)?;
        Ok(Snapshot {
            revision: Digest::of(&bytes),
            bytes,
        })
    }

    /// Capture requested documents and local asset tokens within the project limits.
    /// Bytes are loaded before taking the writer lock for the final revision check.
    pub fn read_set(&self, ids: &[&str]) -> Result<ValidatedReadSet> {
        self.read_set_with_buffer_limit(ids, self.limits.max_total_bytes)
    }

    /// Capture validated document snapshots within an additional aggregate byte budget.
    /// Native assets count against the total read-set limit without using the buffer budget.
    pub fn read_set_with_buffer_limit(
        &self,
        ids: &[&str],
        max_buffered_bytes: u64,
    ) -> Result<ValidatedReadSet> {
        self.check_read_count(ids)?;
        self.check_revision()?;
        let set = self.capture_read_set(ids, Some(max_buffered_bytes))?;
        self.validate_read_set(&set)?;
        Ok(set)
    }

    /// Capture exact local byte revisions without buffering or decoding documents.
    /// Hash once under the cooperating writer lock to keep the selected set coherent.
    pub fn read_revisions(&self, ids: &[&str]) -> Result<ValidatedReadSet> {
        self.check_read_count(ids)?;
        let _lock = write_lock(&self.root)?;
        self.ensure_no_pending()?;
        self.check_revision()?;
        self.capture_read_set(ids, None)
    }

    fn check_read_count(&self, ids: &[&str]) -> Result<()> {
        check_limit(
            "read set resources",
            ids.len() as u64,
            self.limits.max_resources as u64,
        )
    }

    fn capture_read_set(
        &self,
        ids: &[&str],
        mut buffered_remaining: Option<u64>,
    ) -> Result<ValidatedReadSet> {
        let mut documents = BTreeMap::new();
        let mut expected = BTreeMap::new();
        let mut remaining = self.limits.max_total_bytes;
        for &id in ids {
            if expected.contains_key(id) {
                return Err(Error::Validation(format!(
                    "duplicate read set resource {id}"
                )));
            }
            let resource = self.resource(id).ok_or_else(|| Error::Missing(id.into()))?;
            let (size, revision) = if let (Resource::Document(entry), Some(buffer_left)) =
                (resource, buffered_remaining.as_mut())
            {
                let document = self.read_document_bounded(entry, remaining.min(*buffer_left))?;
                let size = document.bytes.len() as u64;
                *buffer_left -= size;
                let revision = document.revision.clone();
                documents.insert(id.to_owned(), document);
                (size, revision)
            } else {
                self.resource_digest(resource, remaining)?
            };
            remaining -= size;
            expected.insert(id.to_owned(), revision);
        }
        Ok(ValidatedReadSet {
            manifest: self.revision.clone(),
            expected,
            documents,
        })
    }

    /// Recheck the manifest and the requested resource tokens under the writer lock.
    /// A successful check establishes a coherent snapshot for cooperating writers.
    pub fn validate_read_set(&self, set: &ValidatedReadSet) -> Result<()> {
        transaction::revision::verify_revision(
            self.revision.clone(),
            &set.manifest,
            "manifest revision",
        )?;
        let _lock = write_lock(&self.root)?;
        self.ensure_no_pending()?;
        self.check_revision()?;
        transaction::revision::verify_expected(self, &set.expected, None)
    }

    /// Open a native asset without buffering its content.
    pub fn open_asset(&self, id: &str) -> Result<File> {
        let index = self
            .assets
            .get(id)
            .ok_or_else(|| Error::Missing(id.into()))?;
        let entry = &self.manifest.assets[*index];
        let file = open_regular(&self.root, &entry.path)?;
        check_limit(
            "asset bytes",
            file.metadata()
                .map_err(|error| io(entry.path.to_string(), error))?
                .len(),
            self.limits.max_asset_bytes,
        )?;
        Ok(file)
    }

    /// Validate registered document/container metadata and check that all local files exist.
    pub fn validate_resources(&self) -> Result<()> {
        ensure_manifest_path(&self.manifest, &self.manifest_path)?;
        for entry in &self.manifest.documents {
            self.read_entry(entry)?;
        }
        for entry in &self.manifest.assets {
            verify_file(&self.root, &entry.path, self.limits.max_asset_bytes)?;
        }
        Ok(())
    }

    /// Replace one document with no hard dependencies or separate declared reads.
    /// Use a change set and apply when the edit depends on other local resources.
    pub fn save_document(
        &self,
        id: &str,
        expected_revision: &Digest,
        bytes: &[u8],
        validate_change: impl FnOnce(&crate::DocumentEntry, &[u8]) -> std::result::Result<(), String>,
    ) -> Result<Digest> {
        let outcome =
            self.save_document_with_outcome(id, expected_revision, bytes, validate_change)?;
        if let Some(error) = outcome.completion_error {
            return Err(error);
        }
        Ok(outcome
            .resources
            .into_iter()
            .next()
            .and_then(|change| change.revision)
            .unwrap_or_else(|| expected_revision.clone()))
    }

    /// Replace one document and report publication, its resource revision, and any
    /// synchronization failure separately.
    pub fn save_document_with_outcome(
        &self,
        id: &str,
        expected_revision: &Digest,
        bytes: &[u8],
        validate_change: impl FnOnce(&crate::DocumentEntry, &[u8]) -> std::result::Result<(), String>,
    ) -> Result<TransactionOutcome> {
        Ok(self.save_document_report(id, expected_revision, bytes, validate_change)?)
    }

    /// Replace one document with structured publication and rejection diagnostics.
    pub fn save_document_report(
        &self,
        id: &str,
        expected_revision: &Digest,
        bytes: &[u8],
        validate_change: impl FnOnce(&crate::DocumentEntry, &[u8]) -> std::result::Result<(), String>,
    ) -> std::result::Result<TransactionOutcome, TransactionRejection> {
        let entry = self.document(id)?;
        let mut changes = ChangeSet::new(self.revision.clone());
        changes.expect(id, expected_revision.clone());
        changes.write(ResourceWrite::new(
            id,
            entry.path.clone(),
            WriteMode::Replace,
            bytes,
        ));
        let mut validate = Some(validate_change);
        let published = transaction::commit::execute(
            self,
            changes,
            &CodecRegistry::new(),
            |_, entry, bytes, _| {
                validate.take().expect("one in-place validation")(entry, bytes)
                    .map_err(Error::Validation)
            },
        )?;
        Ok(published.outcome)
    }

    /// Apply an explicit staged transaction using registered domain codecs.
    pub fn apply<R: Read>(
        &mut self,
        changes: ChangeSet<R>,
        registry: &CodecRegistry,
    ) -> Result<()> {
        let outcome = self.apply_with_outcome(changes, registry)?;
        outcome.completion_error.map_or(Ok(()), Err)
    }

    /// Apply using the shared transaction policy and report the persisted outcome.
    /// An outcome with a completion diagnostic has already advanced the project.
    pub fn apply_with_outcome<R: Read>(
        &mut self,
        changes: ChangeSet<R>,
        registry: &CodecRegistry,
    ) -> Result<TransactionOutcome> {
        Ok(self.apply_report(changes, registry)?)
    }

    /// Apply a transaction. A rejection identifies whether the retained staging
    /// needs inspection. An outcome always reports whether publication occurred.
    pub fn apply_report<R: Read>(
        &mut self,
        changes: ChangeSet<R>,
        registry: &CodecRegistry,
    ) -> std::result::Result<TransactionOutcome, TransactionRejection> {
        let published = transaction::commit::execute(
            self,
            changes,
            registry,
            |mode, entry, bytes, decoded| {
                registry.validate_decoded_write(entry, bytes, decoded, mode == WriteMode::Migrate)
            },
        )?;
        if let Some((manifest, revision)) = published.manifest {
            *self = Self::from_parts(
                self.root.clone(),
                self.manifest_path.clone(),
                manifest,
                revision,
                self.limits,
            );
        }
        Ok(published.outcome)
    }

    /// Update metadata using explicit local resource revision tokens. File operations
    /// such as relocation or replacement require a change set.
    pub fn commit_metadata(
        &mut self,
        next: Manifest,
        reads: &ValidatedReadSet,
    ) -> Result<TransactionOutcome> {
        let mut changes = ChangeSet::<&[u8]>::new(self.revision.clone());
        changes.include_read_set(reads)?;
        changes.set_manifest(next);
        self.apply_with_outcome(changes, &CodecRegistry::new())
    }

    /// Compatibility entry point for manifest metadata with no local registration edits.
    /// Use commit_metadata for metadata on registered local resources or apply for bytes.
    pub fn commit<R: Read>(&mut self, next: Manifest, files: Vec<StagedFile<R>>) -> Result<()> {
        if !files.is_empty() {
            self.check_revision()?;
            return Err(Error::Validation(
                "resource changes require a ChangeSet, affected revisions and codec registry"
                    .into(),
            ));
        }
        let same_local_entries = self.manifest.documents.len() == next.documents.len()
            && self.manifest.assets.len() == next.assets.len()
            && next
                .documents
                .iter()
                .map(Resource::Document)
                .chain(next.assets.iter().map(Resource::Asset))
                .all(|resource| self.resource(resource.id()) == Some(resource));
        if !same_local_entries {
            self.check_revision()?;
            return Err(Error::Validation(
                "local registration changes require commit_metadata with read revisions or a ChangeSet".into(),
            ));
        }
        let mut changes = ChangeSet::<R>::new(self.revision.clone());
        changes.set_manifest(next);
        self.apply(changes, &CodecRegistry::new())
    }

    /// Compute a bounded byte revision for a registered resource without decoding it.
    pub fn resource_revision(&self, id: &str) -> Result<Digest> {
        let resource = self.resource(id).ok_or_else(|| Error::Missing(id.into()))?;
        self.resource_digest(resource, u64::MAX)
            .map(|(_, digest)| digest)
    }

    fn resource_digest(&self, resource: Resource<'_>, remaining: u64) -> Result<(u64, Digest)> {
        let path = resource.path().ok_or_else(|| {
            Error::Validation(format!(
                "external resource {} has no revision",
                resource.id()
            ))
        })?;
        let limit = match resource {
            Resource::Document(_) => self.limits.max_json_bytes,
            _ => self.limits.max_asset_bytes,
        };
        hash_reader(open_regular(&self.root, path)?, limit.min(remaining))
    }

    /// Read the recovery state while coordinating with cooperating writers.
    pub fn inspect_recovery(&self) -> Result<transaction::RecoveryState> {
        let _lock = write_lock(&self.root)?;
        transaction::recovery::inspect(&self.root, &self.manifest_path, &self.limits)
    }

    /// Remove private staging. Retain installed orphan candidates and divergent journals for review.
    pub fn recover(&self) -> Result<transaction::RecoveryReport> {
        let _lock = write_lock(&self.root)?;
        transaction::recovery::recover(&self.root, &self.manifest_path, &self.limits)
    }

    pub(crate) fn ensure_no_pending(&self) -> Result<()> {
        transaction::recovery::ensure_clean(&self.root)
    }
    pub(crate) fn manifest_path(&self) -> &ProjectPath {
        &self.manifest_path
    }

    pub(crate) fn resource(&self, id: &str) -> Option<Resource<'_>> {
        self.documents
            .get(id)
            .map(|&index| Resource::Document(&self.manifest.documents[index]))
            .or_else(|| {
                self.assets
                    .get(id)
                    .map(|&index| Resource::Asset(&self.manifest.assets[index]))
            })
            .or_else(|| {
                self.libraries
                    .get(id)
                    .map(|&index| Resource::ExternalLibrary(&self.manifest.libraries[index]))
            })
    }

    pub(crate) fn check_revision(&self) -> Result<()> {
        if Digest::of(&read_file(
            &self.root,
            &self.manifest_path,
            self.limits.max_json_bytes,
        )?) != self.revision
        {
            return Err(Error::Conflict(self.manifest_path.to_string()));
        }
        Ok(())
    }

    fn document(&self, id: &str) -> Result<&crate::DocumentEntry> {
        self.documents
            .get(id)
            .map(|&index| &self.manifest.documents[index])
            .ok_or_else(|| Error::Missing(id.into()))
    }

    fn read_entry(&self, entry: &crate::DocumentEntry) -> Result<()> {
        let bytes = read_file(&self.root, &entry.path, self.limits.max_json_bytes)?;
        decode_document(entry, &bytes, &self.limits)?;
        Ok(())
    }
}

fn manifest_location(path: &Path) -> Result<(PathBuf, ProjectPath)> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Path(path.display().to_string()))?;
    let name = ProjectPath::new(name)?;
    if name.extension() != Some("cirproj") {
        return Err(Error::Path(name.to_string()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((trusted_root(parent)?, name))
}

pub(crate) fn ensure_manifest_path(manifest: &Manifest, name: &ProjectPath) -> Result<()> {
    crate::path::unique_paths(
        std::iter::once(name)
            .chain(manifest.documents.iter().map(|entry| &entry.path))
            .chain(manifest.assets.iter().map(|entry| &entry.path)),
    )
}
