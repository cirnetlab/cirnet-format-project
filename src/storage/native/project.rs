//! Local filesystem adapter. The manifest is the commit point for staged multi-file changes.

use super::{
    open_regular, persist_new, read_file, temp_bytes, trusted_root, verify_file, write_lock,
};
use crate::{
    CodecRegistry, Digest, Error, Limits, Manifest, ProjectPath, Resource, Result, decode_document,
    decode_manifest,
    error::{check_limit, io},
    hash_reader,
    transaction::{self, ChangeSet, ResourceWrite, WriteMode},
};
use std::{
    collections::HashMap,
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

pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub revision: Digest,
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
        let bytes = manifest.to_bytes(&limits)?;
        let (root, manifest_path) = manifest_location(path.as_ref())?;
        ensure_manifest_path(&manifest, &manifest_path)?;
        let _lock = write_lock(&root)?;
        transaction::recovery::ensure_clean(&root)?;
        let project = Self::from_parts(root, manifest_path, manifest, Digest::of(&bytes), limits);
        project.validate_resources()?;
        let file = temp_bytes(&project.root, &bytes)?;
        persist_new(file, &project.root.join(project.manifest_path.as_path()))?;
        Ok(project)
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
        let entry = self.document(id)?;
        let bytes = read_file(&self.root, &entry.path, self.limits.max_json_bytes)?;
        decode_document(entry, &bytes, &self.limits)?;
        Ok(Snapshot {
            revision: Digest::of(&bytes),
            bytes,
        })
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

    /// Replace one document under the transaction policy. Use apply for declared dependencies.
    pub fn save_document(
        &self,
        id: &str,
        expected_revision: &Digest,
        bytes: &[u8],
        validate_change: impl FnOnce(&crate::DocumentEntry, &[u8]) -> std::result::Result<(), String>,
    ) -> Result<Digest> {
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
        published.completion?;
        Ok(published.document.expect("one in-place resource revision"))
    }

    /// Apply an explicit staged transaction using registered domain codecs.
    pub fn apply<R: Read>(
        &mut self,
        changes: ChangeSet<R>,
        registry: &CodecRegistry,
    ) -> Result<()> {
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
        published.completion
    }

    /// Compatibility entry point for a manifest-only update. Resource edits require apply.
    pub fn commit<R: Read>(&mut self, next: Manifest, files: Vec<StagedFile<R>>) -> Result<()> {
        self.check_revision()?;
        if !files.is_empty() {
            return Err(Error::Validation(
                "resource changes require a ChangeSet, affected revisions and codec registry"
                    .into(),
            ));
        }
        let mut changes = ChangeSet::<R>::new(self.revision.clone());
        changes.set_manifest(next);
        self.apply(changes, &CodecRegistry::new())
    }

    /// Compute a bounded byte revision for a registered resource without decoding it.
    pub fn resource_revision(&self, id: &str) -> Result<Digest> {
        let resource = self.resource(id).ok_or_else(|| Error::Missing(id.into()))?;
        let path = resource
            .path()
            .ok_or_else(|| Error::Validation(format!("external resource {id} has no revision")))?;
        let limit = match resource {
            Resource::Document(_) => self.limits.max_json_bytes,
            _ => self.limits.max_asset_bytes,
        };
        hash_reader(open_regular(&self.root, path)?, limit).map(|(_, digest)| digest)
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
