use super::sync_dir;
use crate::{Error, Result, error::io, integrity::transfer};
use std::{fs, io::Write, path::Path};
use tempfile::NamedTempFile;

pub(crate) fn publish_new_resource(source: &Path, target: &Path, bytes: u64) -> Result<()> {
    if cfg!(unix) {
        match fs::hard_link(source, target) {
            Ok(()) => return sync_installed(target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(Error::Conflict(target.display().to_string()));
            }
            Err(error)
                if cross_volume(&error) || error.kind() == std::io::ErrorKind::Unsupported => {}
            Err(error) => return Err(io(target.display().to_string(), error)),
        }
    }
    persist_new(copy_staged(source, target, bytes)?, target)
}

/// Return post-install sync errors separately from failures before replacement.
pub(crate) fn replace_staged(staged: &Path, destination: &Path) -> Result<Result<()>> {
    if !super::no_link_metadata(destination)?.is_some_and(|metadata| metadata.is_file()) {
        return Err(Error::Conflict(destination.display().to_string()));
    }
    match fs::rename(staged, destination) {
        Ok(()) => {}
        Err(error) if cross_volume(&error) => {
            let length = fs::metadata(staged)
                .map_err(|error| io(staged.display().to_string(), error))?
                .len();
            let copy = copy_staged(staged, destination, length)?;
            copy.persist(destination)
                .map_err(|error| io(destination.display().to_string(), error.error))?;
        }
        Err(error) => return Err(io(destination.display().to_string(), error)),
    }
    Ok(sync_installed(destination))
}

pub(crate) fn replace_temp(file: NamedTempFile, destination: &Path) -> Result<Result<()>> {
    if !super::no_link_metadata(destination)?.is_some_and(|metadata| metadata.is_file()) {
        return Err(Error::Conflict(destination.display().to_string()));
    }
    file.persist(destination)
        .map_err(|error| io(destination.display().to_string(), error.error))?;
    Ok(sync_installed(destination))
}

fn cross_volume(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices
        || (cfg!(windows) && error.raw_os_error() == Some(17))
}

fn copy_staged(source: &Path, target: &Path, bytes: u64) -> Result<NamedTempFile> {
    let parent = target.parent().expect("project resource has parent");
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|error| io(target.display().to_string(), error))?;
    let input = fs::File::open(source).map_err(|error| io(source.display().to_string(), error))?;
    let (count, _) = transfer(
        input,
        temporary.as_file_mut(),
        bytes,
        "publish staged resource",
    )?;
    if count != bytes {
        return Err(Error::Integrity(target.display().to_string()));
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| io(target.display().to_string(), error))?;
    Ok(temporary)
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

fn sync_installed(destination: &Path) -> Result<()> {
    sync_dir(destination.parent().expect("destination has parent")).map_err(|error| match error {
        Error::Io { source, .. } => Error::Durability {
            context: destination.display().to_string(),
            source,
        },
        other => other,
    })
}
