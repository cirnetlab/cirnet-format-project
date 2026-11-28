//! Local filesystem adapter. The manifest is the commit point for staged multi-file changes.

use crate::{
    Digest, DocumentRead, Error, Limits, Manifest, ProjectPath, Resource, Result, decode_document,
    decode_manifest,
};
use crate::{
    error::{check_limit, io},
    integrity::transfer,
};
use std::{
    collections::HashMap,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

const LOCK_NAME: &str = ".cirnet-write.lock";

/// An independently opened manifest snapshot, with an optimistic revision token.
pub struct Project {
    root: PathBuf,
    manifest_path: ProjectPath,
    manifest: Manifest,
    revision: Digest,
    limits: Limits,
    documents: HashMap<String, usize>,
    assets: HashMap<String, usize>,
}

pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub revision: Digest,
}

/// New bytes for a registered immutable path in a staged commit.
pub struct StagedFile<R> {
    path: ProjectPath,
    reader: R,
}

impl<R: Read> StagedFile<R> {
    pub fn new(path: ProjectPath, reader: R) -> Self {
        Self { path, reader }
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
        Self {
            root,
            manifest_path,
            manifest,
            revision,
            limits,
            documents,
            assets,
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

    /// Replace one document after revision comparison and explicit domain-owner validation.
    /// Unsupported envelopes and unknown kinds may only be preserved at an immutable path.
    pub fn save_document(
        &self,
        id: &str,
        expected_revision: &Digest,
        bytes: &[u8],
        validate_change: impl FnOnce(&crate::DocumentEntry, &[u8]) -> std::result::Result<(), String>,
    ) -> Result<Digest> {
        let entry = self.document(id)?;
        let _lock = write_lock(&self.root)?;
        self.check_revision()?;
        let old = self.read_document(id)?;
        if &old.revision != expected_revision {
            return Err(Error::Conflict(entry.path.to_string()));
        }
        if old.bytes == bytes {
            return Ok(old.revision);
        }
        let before = decode_document(entry, &old.bytes, &self.limits)?;
        let after = decode_document(entry, bytes, &self.limits)?;
        match (before, after) {
            (DocumentRead::Envelope(old), DocumentRead::Envelope(new))
                if old
                    .extensions
                    .iter()
                    .all(|(key, value)| new.extensions.get(key) == Some(value)) => {}
            (DocumentRead::LegacyCircuit(_), DocumentRead::LegacyCircuit(_)) => (),
            (DocumentRead::Envelope(_), DocumentRead::Envelope(_)) => {
                return Err(Error::Validation(
                    "document extension data would be changed or lost".into(),
                ));
            }
            _ => {
                return Err(Error::Unsupported {
                    domain: "document write",
                    version: entry.schema_version.clone(),
                });
            }
        }
        validate_change(entry, bytes).map_err(Error::Validation)?;
        let destination = resolve(&self.root, &entry.path, false)?;
        let parent = destination.parent().expect("document has a project parent");
        let file = temp_bytes(parent, bytes)?;
        if Digest::of(&read_file(
            &self.root,
            &entry.path,
            self.limits.max_json_bytes,
        )?) != *expected_revision
        {
            return Err(Error::Conflict(entry.path.to_string()));
        }
        persist_replace(file, &destination)?;
        Ok(Digest::of(bytes))
    }

    /// Commit a new manifest with optional new immutable files. The previous manifest stays
    /// authoritative until the last atomic write. Previously registered bytes are never overwritten.
    pub fn commit<R: Read>(&mut self, next: Manifest, files: Vec<StagedFile<R>>) -> Result<()> {
        let bytes = next.to_bytes(&self.limits)?;
        ensure_manifest_path(&next, &self.manifest_path)?;
        if self.manifest.project_id != next.project_id {
            return Err(Error::Validation(
                "a save cannot change its project ID".into(),
            ));
        }
        let _lock = write_lock(&self.root)?;
        self.check_revision()?;
        let resources: HashMap<_, _> = next
            .documents
            .iter()
            .map(|doc| (&doc.path, Resource::Document(doc)))
            .chain(
                next.assets
                    .iter()
                    .map(|asset| (&asset.path, Resource::Asset(asset))),
            )
            .collect();
        let mut prepared = HashMap::new();
        let mut total = 0u64;
        check_limit(
            "staged resource count",
            files.len() as u64,
            self.limits.max_resources as u64,
        )?;
        for staged in files {
            let resource = resources.get(&staged.path).ok_or_else(|| {
                Error::Validation(format!("unregistered staged file {}", staged.path))
            })?;
            if prepared.contains_key(&staged.path) {
                return Err(Error::Conflict(staged.path.to_string()));
            }
            let destination = resolve(&self.root, &staged.path, true)?;
            if no_link_metadata(&destination)?.is_some() {
                return Err(Error::Conflict(staged.path.to_string()));
            }
            let mut temp =
                NamedTempFile::new_in(destination.parent().expect("project path has parent"))
                    .map_err(|error| io(staged.path.to_string(), error))?;
            let limit = match resource {
                Resource::Document(_) => self.limits.max_json_bytes,
                _ => self.limits.max_asset_bytes,
            };
            let (count, _) = transfer(
                staged.reader,
                temp.as_file_mut(),
                limit.min(self.limits.max_total_bytes.saturating_sub(total)),
                staged.path.as_str(),
            )?;
            total += count;
            if let Resource::Document(entry) = resource {
                temp.rewind()
                    .map_err(|error| io(staged.path.to_string(), error))?;
                let document = read_bounded(
                    temp.as_file_mut(),
                    self.limits.max_json_bytes,
                    staged.path.as_str(),
                )?;
                decode_document(entry, &document, &self.limits)?;
            }
            temp.as_file()
                .sync_all()
                .map_err(|error| io(staged.path.to_string(), error))?;
            prepared.insert(staged.path, (destination, temp.into_temp_path()));
        }

        let previous_documents: HashMap<_, _> = self
            .manifest
            .documents
            .iter()
            .map(|entry| (entry.id.as_str(), entry))
            .collect();
        for entry in &next.documents {
            if prepared.contains_key(&entry.path) {
                continue;
            }
            if previous_documents
                .get(entry.id.as_str())
                .is_some_and(|old| {
                    old.path == entry.path
                        && old.kind == entry.kind
                        && old.schema_version == entry.schema_version
                })
            {
                verify_file(&self.root, &entry.path, self.limits.max_json_bytes)?;
            } else {
                self.read_entry(entry)?;
            }
        }
        for entry in &next.assets {
            if !prepared.contains_key(&entry.path) {
                verify_file(&self.root, &entry.path, self.limits.max_asset_bytes)?;
            }
        }
        let manifest = temp_bytes(&self.root, &bytes)?;
        for (_, (destination, temp)) in prepared {
            persist_new_path(temp, &destination)?;
        }
        self.check_revision()?;
        let result = persist_replace(manifest, &self.root.join(self.manifest_path.as_path()));
        if result.is_ok() || matches!(&result, Err(Error::Durability { .. })) {
            *self = Self::from_parts(
                self.root.clone(),
                self.manifest_path.clone(),
                next,
                Digest::of(&bytes),
                self.limits,
            );
        }
        result
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

fn ensure_manifest_path(manifest: &Manifest, name: &ProjectPath) -> Result<()> {
    crate::path::unique_paths(
        std::iter::once(name)
            .chain(manifest.documents.iter().map(|entry| &entry.path))
            .chain(manifest.assets.iter().map(|entry| &entry.path)),
    )
}

pub(crate) fn trusted_root(path: &Path) -> Result<PathBuf> {
    let absolute =
        std::path::absolute(path).map_err(|error| io(path.display().to_string(), error))?;
    if !no_link_metadata(&absolute)?.is_some_and(|metadata| metadata.is_dir()) {
        return Err(Error::Path(absolute.display().to_string()));
    }
    fs::canonicalize(&absolute).map_err(|error| io(absolute.display().to_string(), error))
}

pub(crate) fn no_link_metadata(path: &Path) -> Result<Option<Metadata>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(path.display().to_string(), error)),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(Error::Path(path.display().to_string()));
        }
    }
    if meta.file_type().is_symlink() {
        return Err(Error::Path(path.display().to_string()));
    }
    Ok(Some(meta))
}

pub(crate) fn resolve(root: &Path, path: &ProjectPath, create: bool) -> Result<PathBuf> {
    let mut absolute = root.to_owned();
    let mut parts = path.as_str().split('/').peekable();
    while let Some(part) = parts.next() {
        absolute.push(part);
        if parts.peek().is_none() {
            break;
        }
        let exists = no_link_metadata(&absolute)?;
        if let Some(meta) = exists {
            if !meta.is_dir() {
                return Err(Error::Path(path.to_string()));
            }
        } else if create {
            match fs::create_dir(&absolute) {
                Ok(()) => sync_dir(absolute.parent().expect("has parent"))?,
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists
                        && no_link_metadata(&absolute)?.is_some_and(|meta| meta.is_dir()) => {}
                Err(error) => return Err(io(path.to_string(), error)),
            }
        } else {
            return Err(Error::Missing(path.to_string()));
        }
    }
    Ok(absolute)
}

pub(crate) fn open_regular(root: &Path, path: &ProjectPath) -> Result<File> {
    let destination = resolve(root, path, false)?;
    let metadata =
        no_link_metadata(&destination)?.ok_or_else(|| Error::Missing(path.to_string()))?;
    if !metadata.is_file() {
        return Err(Error::Path(path.to_string()));
    }
    let file = File::open(&destination).map_err(|error| io(path.to_string(), error))?;
    if !file
        .metadata()
        .map_err(|error| io(path.to_string(), error))?
        .is_file()
    {
        return Err(Error::Path(path.to_string()));
    }
    Ok(file)
}

fn verify_file(root: &Path, path: &ProjectPath, limit: u64) -> Result<()> {
    let file = open_regular(root, path)?;
    check_limit(
        "resource bytes",
        file.metadata()
            .map_err(|error| io(path.to_string(), error))?
            .len(),
        limit,
    )
}

pub(crate) fn read_bounded(reader: impl Read, limit: u64, context: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io(context, error))?;
    check_limit("file bytes", bytes.len() as u64, limit)?;
    Ok(bytes)
}

pub(crate) fn read_file(root: &Path, path: &ProjectPath, limit: u64) -> Result<Vec<u8>> {
    read_bounded(open_regular(root, path)?, limit, path.as_str())
}

pub(crate) fn write_lock(root: &Path) -> Result<File> {
    let path = root.join(LOCK_NAME);
    if no_link_metadata(&path)?.is_some_and(|meta| !meta.is_file()) {
        return Err(Error::Path(path.display().to_string()));
    }
    let guard = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| io(path.display().to_string(), error))?;
    guard
        .lock()
        .map_err(|error| io(path.display().to_string(), error))?;
    Ok(guard)
}

pub(crate) fn temp_bytes(parent: &Path, bytes: &[u8]) -> Result<NamedTempFile> {
    let context = parent.display().to_string();
    let mut file = NamedTempFile::new_in(parent).map_err(|error| io(&context, error))?;
    file.write_all(bytes).map_err(|error| io(&context, error))?;
    file.as_file()
        .sync_all()
        .map_err(|error| io(context, error))?;
    Ok(file)
}

pub(crate) fn persist_new(file: NamedTempFile, destination: &Path) -> Result<()> {
    persist_new_path(file.into_temp_path(), destination)
}

pub(crate) fn persist_new_path(file: tempfile::TempPath, destination: &Path) -> Result<()> {
    file.persist_noclobber(destination).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Conflict(destination.display().to_string())
        } else {
            io(destination.display().to_string(), error.error)
        }
    })?;
    sync_installed(destination)
}

fn persist_replace(file: NamedTempFile, destination: &Path) -> Result<()> {
    if !no_link_metadata(destination)?.is_some_and(|meta| meta.is_file()) {
        return Err(Error::Conflict(destination.display().to_string()));
    }
    file.persist(destination)
        .map_err(|error| io(destination.display().to_string(), error.error))?;
    sync_installed(destination)
}

fn sync_installed(destination: &Path) -> Result<()> {
    sync_dir(destination.parent().expect("destination has parent")).map_err(|error| match error {
        Error::Io { source, .. } => Error::Durability {
            context: destination.display().to_string(),
            source,
        },
        other => other,
    })
}

pub(crate) fn sync_dir(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| io(directory.display().to_string(), error))?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
