use crate::{
    Error, ProjectPath, Result,
    error::{check_limit, io},
};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};
const LOCK_NAME: &str = ".cirnet-write.lock";

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

pub(crate) fn verify_file(root: &Path, path: &ProjectPath, limit: u64) -> Result<()> {
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

pub(crate) fn sync_dir(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| io(directory.display().to_string(), error))?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
