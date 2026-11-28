use super::{Item, MANIFEST_ENTRY, native::destination_parts};
use crate::{
    Error, ProjectPath, Result,
    error::io,
    integrity::hash_reader,
    storage::{Project, no_link_metadata, sync_dir, trusted_root},
    transaction::DurabilityStatus,
};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

const MARKER: &str = ".cirnet-incomplete-import";
const MARKER_PREFIX: &str = "cirnet.package pending import\n";
const MARKER_LIMIT: u64 = 128;

/// Reports a completed manifest installation, including any warning after the commit point.
pub struct PackagePublishOutcome {
    pub project: Project,
    /// A durability or marker cleanup error after publication. The project is already visible.
    pub warning: Option<Error>,
}

impl PackagePublishOutcome {
    /// Confirmation of the filesystem synchronization available for this import.
    pub fn durability(&self) -> DurabilityStatus {
        DurabilityStatus::after_publication(self.warning.as_ref())
    }

    /// Inspect the published destination when synchronization or marker cleanup failed.
    pub fn inspection_required(&self) -> bool {
        self.warning.is_some()
    }
}

/// Precommit outcomes from verified package publication.
#[derive(Debug, thiserror::Error)]
pub enum PackagePublishError {
    /// No manifest was installed and this attempt left no destination content.
    #[error(transparent)]
    Rejected(#[from] Error),
    /// No manifest was installed by this attempt. Retained content requires inspection.
    #[error("incomplete package import at {destination:?}: {cause}; cleanup: {cleanup}")]
    Incomplete {
        destination: PathBuf,
        #[source]
        cause: Box<Error>,
        cleanup: Box<Error>,
    },
}

impl PackagePublishError {
    /// Check the incomplete destination before retrying.
    pub fn inspection_required(&self) -> bool {
        matches!(self, Self::Incomplete { .. })
    }
}

impl From<PackagePublishError> for Error {
    fn from(error: PackagePublishError) -> Self {
        match error {
            PackagePublishError::Rejected(cause) => cause,
            PackagePublishError::Incomplete {
                destination,
                cause,
                cleanup,
            } => Self::IncompletePackageImport {
                destination,
                cause,
                cleanup,
            },
        }
    }
}

/// Read-only destination classification under a trusted local parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PackageDestinationState {
    Available,
    PendingImport,
    ManifestPresent,
    Occupied,
}

/// Check for a manifest or the published import marker. This does not validate file content
/// or establish ownership for automatic cleanup after a process interruption.
pub fn inspect_package_destination(
    destination: impl AsRef<Path>,
) -> Result<PackageDestinationState> {
    use PackageDestinationState::*;
    let (parent, name) = destination_parts(destination.as_ref())?;
    let root = trusted_root(parent)?.join(name.as_path());
    match no_link_metadata(&root)? {
        None => return Ok(Available),
        Some(metadata) if !metadata.is_dir() => return Ok(Occupied),
        Some(_) => {}
    }
    if no_link_metadata(&root.join(MANIFEST_ENTRY))?.is_some_and(|metadata| metadata.is_file()) {
        return Ok(ManifestPresent);
    }
    let marker = root.join(MARKER);
    if !no_link_metadata(&marker)?
        .is_some_and(|metadata| metadata.is_file() && metadata.len() <= MARKER_LIMIT)
    {
        return Ok(Occupied);
    }
    let bytes = marker_bytes(&marker)?;
    let recognized = bytes
        .strip_prefix(MARKER_PREFIX.as_bytes())
        .is_some_and(|nonce| {
            !nonce.is_empty()
                && nonce
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.')
        });
    Ok(if recognized { PendingImport } else { Occupied })
}

struct Installed<'a> {
    metadata: Metadata,
    expected: &'a Item,
}

struct Marker {
    metadata: Metadata,
    bytes: Vec<u8>,
}

/// Tracks files and directories created by this live operation. A journal marker alone
/// is deliberately insufficient authority for rollback across process restarts.
pub(super) struct Destination<'a> {
    pub(super) root: PathBuf,
    root_metadata: Option<Metadata>,
    dirs: BTreeMap<PathBuf, Metadata>,
    files: BTreeMap<PathBuf, Installed<'a>>,
    marker: Option<Marker>,
}

impl<'a> Destination<'a> {
    pub(super) fn create(root: PathBuf) -> Result<Self> {
        fs::create_dir(&root).map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists => conflict(&root),
            _ => io(root.display().to_string(), error),
        })?;
        Ok(Self {
            root,
            root_metadata: None,
            dirs: BTreeMap::new(),
            files: BTreeMap::new(),
            marker: None,
        })
    }

    pub(super) fn start(&mut self) -> Result<()> {
        self.root_metadata = no_link_metadata(&self.root)?;
        if !self.root_metadata.as_ref().is_some_and(Metadata::is_dir) {
            return Err(conflict(&self.root));
        }
        let marker = self.root.join(MARKER);
        let mut temp = NamedTempFile::new_in(&self.root).map_err(|error| io(MARKER, error))?;
        let nonce = temp
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("ASCII temp name");
        let bytes = format!("{MARKER_PREFIX}{nonce}").into_bytes();
        temp.write_all(&bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|error| io(MARKER, error))?;
        let metadata = install_file(temp, &marker)?;
        self.marker = Some(Marker { metadata, bytes });
        sync_dir(&self.root)?;
        sync_dir(self.root.parent().expect("import root has parent"))
    }

    fn ensure_root(&self) -> Result<()> {
        let metadata = self
            .root_metadata
            .as_ref()
            .ok_or_else(|| conflict(&self.root))?;
        ensure_object(&self.root, metadata)?;
        Ok(())
    }

    pub(super) fn ensure_path(&self, target: &Path) -> Result<()> {
        self.ensure_root()?;
        let parent = target.parent().expect("import target has a parent");
        let relative = parent
            .strip_prefix(&self.root)
            .expect("import target is relative");
        let mut directory = self.root.clone();
        for component in relative.components() {
            directory.push(component);
            let expected = self
                .dirs
                .get(&directory)
                .ok_or_else(|| conflict(&directory))?;
            ensure_object(&directory, expected)?;
        }
        Ok(())
    }

    pub(super) fn resource_path(&mut self, name: &ProjectPath) -> Result<PathBuf> {
        self.ensure_root()?;
        let target = self.root.join(name.as_path());
        let mut cursor = self.root.clone();
        let parent = name.as_path().parent().expect("relative member");
        for part in parent.components() {
            cursor.push(part);
            if let Some(expected) = self.dirs.get(&cursor) {
                ensure_object(&cursor, expected)?;
            } else {
                fs::create_dir(&cursor).map_err(|error| match error.kind() {
                    ErrorKind::AlreadyExists => conflict(&cursor),
                    _ => io(cursor.display().to_string(), error),
                })?;
                let metadata = no_link_metadata(&cursor)?
                    .filter(Metadata::is_dir)
                    .ok_or_else(|| conflict(&cursor))?;
                self.dirs.insert(cursor.clone(), metadata);
                sync_dir(cursor.parent().expect("resource directory has parent"))?;
            }
        }
        Ok(target)
    }

    pub(super) fn record(
        &mut self,
        target: PathBuf,
        metadata: Metadata,
        expected: &'a Item,
    ) -> Result<()> {
        self.files
            .insert(target.clone(), Installed { metadata, expected });
        sync_publication_directory(target.parent().expect("resource has parent"), &target)
    }

    pub(super) fn rollback(&self) -> Result<()> {
        let root_metadata = self
            .root_metadata
            .as_ref()
            .ok_or_else(|| conflict(&self.root))?;
        ensure_object(&self.root, root_metadata)?;
        self.check_contents(&self.root)?;
        for (directory, metadata) in &self.dirs {
            ensure_object(directory, metadata)?;
            self.check_contents(directory)?;
        }
        for (resource, installed) in &self.files {
            let current = ensure_object(resource, &installed.metadata)?;
            if current.len() != installed.expected.bytes
                || current.modified().ok() != installed.metadata.modified().ok()
                || hash_reader(
                    File::open(resource)
                        .map_err(|error| io(resource.display().to_string(), error))?,
                    installed.expected.bytes,
                )? != (installed.expected.bytes, installed.expected.sha256.clone())
            {
                return Err(conflict(resource));
            }
        }
        self.check_marker()?;
        for resource in self.files.keys() {
            fs::remove_file(resource).map_err(|error| io(resource.display().to_string(), error))?;
        }
        for directory in self.dirs.keys().rev() {
            fs::remove_dir(directory)
                .map_err(|error| io(directory.display().to_string(), error))?;
        }
        self.remove_marker()?;
        fs::remove_dir(&self.root).map_err(|error| io(self.root.display().to_string(), error))?;
        sync_dir(self.root.parent().expect("import root has parent"))
    }

    fn check_contents(&self, directory: &Path) -> Result<()> {
        for entry in
            fs::read_dir(directory).map_err(|error| io(directory.display().to_string(), error))?
        {
            let path = entry
                .map_err(|error| io(directory.display().to_string(), error))?
                .path();
            if !self.dirs.contains_key(&path)
                && !self.files.contains_key(&path)
                && !(directory == self.root
                    && self.marker.is_some()
                    && path.file_name().is_some_and(|name| name == MARKER))
            {
                return Err(conflict(&path));
            }
        }
        Ok(())
    }

    fn check_marker(&self) -> Result<()> {
        if let Some(expected) = &self.marker {
            let path = self.root.join(MARKER);
            let actual = ensure_object(&path, &expected.metadata)?;
            if actual.len() != expected.bytes.len() as u64 || marker_bytes(&path)? != expected.bytes
            {
                return Err(conflict(&path));
            }
        }
        Ok(())
    }

    pub(super) fn remove_marker(&self) -> Result<()> {
        self.check_marker()?;
        if self.marker.is_some() {
            fs::remove_file(self.root.join(MARKER)).map_err(|error| io(MARKER, error))?;
            sync_dir(&self.root)?;
        }
        Ok(())
    }
}

fn marker_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MARKER_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| io(MARKER, error))?;
    Ok(bytes)
}

fn ensure_object(path: &Path, expected: &Metadata) -> Result<Metadata> {
    no_link_metadata(path)?
        .filter(|current| {
            current.file_type() == expected.file_type() && same_object(current, expected)
        })
        .ok_or_else(|| conflict(path))
}

#[cfg(unix)]
fn same_object(current: &Metadata, expected: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    (current.dev(), current.ino(), current.mode())
        == (expected.dev(), expected.ino(), expected.mode())
}

#[cfg(not(unix))]
fn same_object(current: &Metadata, expected: &Metadata) -> bool {
    // The enclosing directory is caller-controlled. Missing birth times are insufficient proof.
    current
        .created()
        .ok()
        .is_some_and(|created| Some(created) == expected.created().ok())
        && current.permissions().readonly() == expected.permissions().readonly()
}

fn conflict(path: &Path) -> Error {
    Error::Conflict(path.display().to_string())
}

/// Install without clobbering. Synchronization is separated so the caller can record
/// the new directory entry before an I/O error or a cancellation.
pub(super) fn install_file(file: NamedTempFile, target: &Path) -> Result<Metadata> {
    let metadata = file
        .as_file()
        .metadata()
        .map_err(|error| io(target.display().to_string(), error))?;
    file.persist_noclobber(target)
        .map_err(|error| match error.error.kind() {
            ErrorKind::AlreadyExists => conflict(target),
            _ => io(target.display().to_string(), error.error),
        })?;
    Ok(metadata)
}

pub(super) fn sync_publication_directory(directory: &Path, target: &Path) -> Result<()> {
    sync_dir(directory).map_err(|error| match error {
        Error::Io { source, .. } => Error::Durability {
            context: target.display().to_string(),
            source,
        },
        other => other,
    })
}
