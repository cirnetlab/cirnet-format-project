use crate::storage::{no_link_metadata, open_regular, read_file, sync_dir};
use crate::{
    Digest, Error, Limits, ProjectPath, Result,
    error::{check_limit, io},
    hash_reader, json,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const STAGING_DIR: &str = ".cirnet-transaction";
const JOURNAL: &str = "journal.json";
const JOURNAL_TEMP: &str = "journal.partial";
pub(crate) const ANCHOR_STAGE: &str = "commit.stage";
const JOURNAL_LIMIT: u64 = 4 * 1024 * 1024;

/// A read-only interpretation of the journal against the current publication point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryState {
    Clean,
    Staging,
    BeforePublication,
    Published,
    Diverged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    pub state: RecoveryState,
    /// Paths published before the manifest switch and preserved for explicit review.
    pub retained: Vec<RetainedResource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedResource {
    pub path: ProjectPath,
    /// True when size and digest match the journal. This alone does not prove ownership.
    pub matches_staged: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Journal {
    pub version: u8,
    pub manifest: ProjectPath,
    pub before: Digest,
    pub after: Digest,
    pub files: Vec<JournalFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JournalFile {
    pub path: ProjectPath,
    pub digest: Digest,
    pub bytes: u64,
}

pub(crate) fn directory(root: &Path) -> PathBuf {
    root.join(STAGING_DIR)
}
pub(crate) fn data_name(index: usize) -> String {
    format!("write-{index:08}.stage")
}

pub(crate) fn ensure_clean(root: &Path) -> Result<()> {
    if no_link_metadata(&directory(root))?.is_some() {
        return Err(Error::RecoveryRequired);
    }
    Ok(())
}

pub(crate) fn begin(root: &Path) -> Result<PathBuf> {
    let stage = directory(root);
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder
        .create(&stage)
        .map_err(|error| io(stage.display().to_string(), error))?;
    sync_dir(root)?;
    Ok(stage)
}

pub(crate) fn create_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| io(path.display().to_string(), error))
}

fn journal_limits() -> Limits {
    Limits {
        max_json_bytes: JOURNAL_LIMIT,
        ..Limits::default()
    }
}

pub(crate) fn write_journal(stage: &Path, journal: &Journal) -> Result<()> {
    let bytes = json::write(journal, &journal_limits())?;
    let temp = stage.join(JOURNAL_TEMP);
    let mut file = create_file(&temp)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| io(JOURNAL_TEMP, error))?;
    drop(file);
    fs::rename(temp, stage.join(JOURNAL)).map_err(|error| io(JOURNAL, error))?;
    sync_dir(stage)
}

fn status(
    root: &Path,
    manifest: &ProjectPath,
    limits: &Limits,
) -> Result<(RecoveryState, Option<Journal>)> {
    let stage = directory(root);
    match no_link_metadata(&stage)? {
        None => return Ok((RecoveryState::Clean, None)),
        Some(metadata) if metadata.is_dir() => {}
        _ => return Err(Error::Path(stage.display().to_string())),
    }
    let path = stage.join(JOURNAL);
    if no_link_metadata(&path)?.is_none() {
        return Ok((RecoveryState::Staging, None));
    }
    let journal_path = ProjectPath::new(JOURNAL)?;
    let journal: Journal = json::read(
        &read_file(&stage, &journal_path, JOURNAL_LIMIT)?,
        &journal_limits(),
    )?;
    if journal.version != 1 {
        return Err(Error::Unsupported {
            domain: "recovery journal",
            version: journal.version.to_string(),
        });
    }
    check_limit(
        "journal files",
        journal.files.len() as u64,
        limits.max_resources as u64,
    )?;
    crate::path::unique_paths(journal.files.iter().map(|file| &file.path))?;
    let mut total = 0;
    for file in &journal.files {
        check_limit(
            "journal resource bytes",
            file.bytes,
            limits.max_asset_bytes.max(limits.max_json_bytes),
        )?;
        check_limit(
            "journal total bytes",
            file.bytes,
            limits.max_total_bytes.saturating_sub(total),
        )?;
        total += file.bytes;
    }
    if &journal.manifest != manifest
        || journal.before == journal.after
        || journal
            .files
            .iter()
            .any(|file| file.path == journal.manifest)
    {
        return Ok((RecoveryState::Diverged, Some(journal)));
    }
    if !verify_staging(&stage, &journal, limits)? {
        return Ok((RecoveryState::Diverged, Some(journal)));
    }
    let current = match read_file(root, &journal.manifest, limits.max_json_bytes) {
        Ok(bytes) => Digest::of(&bytes),
        Err(Error::Missing(_) | Error::Path(_) | Error::Limit { .. }) => {
            return Ok((RecoveryState::Diverged, Some(journal)));
        }
        Err(error) => return Err(error),
    };
    if current == journal.after {
        for file in &journal.files {
            let actual =
                open_regular(root, &file.path).and_then(|handle| hash_reader(handle, file.bytes));
            if !actual.is_ok_and(|(size, digest)| size == file.bytes && digest == file.digest) {
                return Ok((RecoveryState::Diverged, Some(journal)));
            }
        }
        Ok((RecoveryState::Published, Some(journal)))
    } else if current == journal.before {
        Ok((RecoveryState::BeforePublication, Some(journal)))
    } else {
        Ok((RecoveryState::Diverged, Some(journal)))
    }
}

pub(crate) fn inspect(
    root: &Path,
    manifest: &ProjectPath,
    limits: &Limits,
) -> Result<RecoveryState> {
    status(root, manifest, limits).map(|(state, _)| state)
}

fn stage_index(name: &str) -> Option<usize> {
    let index = name.strip_prefix("write-")?.strip_suffix(".stage")?;
    if index.len() != 8 || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    index.parse().ok()
}

fn owned_stage_file(name: &str) -> bool {
    matches!(name, JOURNAL | JOURNAL_TEMP | ANCHOR_STAGE) || stage_index(name).is_some()
}

fn verify_staging(stage: &Path, journal: &Journal, limits: &Limits) -> Result<bool> {
    // A crash during cleanup can leave only a subset of the expected files.
    // Any surviving staged bytes must still belong to this journal.
    for entry in fs::read_dir(stage).map_err(|error| io(stage.display().to_string(), error))? {
        let entry = entry.map_err(|error| io(stage.display().to_string(), error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Ok(false);
        };
        if name == JOURNAL {
            continue;
        }
        let (expected, bound, size) = if name == ANCHOR_STAGE {
            (&journal.after, limits.max_json_bytes, None)
        } else {
            let Some(file) = stage_index(name).and_then(|index| journal.files.get(index)) else {
                return Ok(false);
            };
            (&file.digest, file.bytes, Some(file.bytes))
        };
        let path = ProjectPath::new(name)?;
        let actual = open_regular(stage, &path).and_then(|handle| hash_reader(handle, bound));
        if !actual
            .is_ok_and(|(bytes, digest)| &digest == expected && size.is_none_or(|n| n == bytes))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn cleanup(root: &Path, limit: usize) -> Result<()> {
    let stage = directory(root);
    let mut found = Vec::new();
    for entry in fs::read_dir(&stage).map_err(|error| io(stage.display().to_string(), error))? {
        check_limit(
            "staged files",
            found.len() as u64 + 1,
            (limit as u64).saturating_add(3),
        )?;
        let entry = entry.map_err(|error| io(stage.display().to_string(), error))?;
        let path = entry.path();
        if !entry.file_name().to_str().is_some_and(owned_stage_file)
            || !no_link_metadata(&path)?.is_some_and(|metadata| metadata.is_file())
        {
            return Err(Error::Conflict(path.display().to_string()));
        }
        found.push(path);
    }
    // Leave the journal in place until every private payload has been removed.
    found.sort_by_key(|path| path.file_name().is_some_and(|name| name == JOURNAL));
    for path in found {
        fs::remove_file(&path).map_err(|error| io(path.display().to_string(), error))?;
    }
    fs::remove_dir(&stage).map_err(|error| io(stage.display().to_string(), error))?;
    sync_dir(root)
}

pub(crate) fn recover(
    root: &Path,
    manifest: &ProjectPath,
    limits: &Limits,
) -> Result<RecoveryReport> {
    let (state, journal) = status(root, manifest, limits)?;
    let mut retained = Vec::new();
    if state == RecoveryState::BeforePublication
        && let Some(journal) = journal
    {
        for file in journal.files {
            match open_regular(root, &file.path) {
                Ok(handle) => {
                    // Digest identifies the bytes in the journal. It does not establish exclusive file ownership.
                    let matches_staged = hash_reader(handle, file.bytes)
                        .is_ok_and(|(size, digest)| size == file.bytes && digest == file.digest);

                    retained.push(RetainedResource {
                        path: file.path,
                        matches_staged,
                    });
                }
                Err(Error::Missing(_)) => (),
                Err(Error::Path(_)) => retained.push(RetainedResource {
                    path: file.path,
                    matches_staged: false,
                }),
                Err(error) => return Err(error),
            }
        }
    }
    if !matches!(state, RecoveryState::Clean | RecoveryState::Diverged) {
        cleanup(root, limits.max_resources)?;
    }
    Ok(RecoveryReport { state, retained })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_distinguishes_complete_publication_from_external_changes() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let limits = Limits::default();
        let anchor = ProjectPath::new("project.cirproj").unwrap();
        let resource = ProjectPath::new("resource.bin").unwrap();
        fs::write(root.join(anchor.as_path()), b"published").unwrap();
        fs::write(root.join(resource.as_path()), b"asset").unwrap();
        let stage = begin(root).unwrap();
        let journal = Journal {
            version: 1,
            manifest: anchor.clone(),
            before: Digest::of(b"previous"),
            after: Digest::of(b"published"),
            files: vec![JournalFile {
                path: resource.clone(),
                digest: Digest::of(b"asset"),
                bytes: 5,
            }],
        };
        write_journal(&stage, &journal).unwrap();
        let bounded = Limits {
            max_total_bytes: 4,
            ..limits
        };
        assert!(matches!(
            inspect(root, &anchor, &bounded),
            Err(Error::Limit {
                resource: "journal total bytes",
                ..
            })
        ));
        assert_eq!(
            inspect(root, &anchor, &limits).unwrap(),
            RecoveryState::Published
        );
        fs::write(root.join(resource.as_path()), b"modified").unwrap();
        assert_eq!(
            recover(root, &anchor, &limits).unwrap().state,
            RecoveryState::Diverged
        );
        assert!(stage.exists());
        assert_eq!(
            fs::read(root.join(resource.as_path())).unwrap(),
            b"modified"
        );
        fs::write(root.join(resource.as_path()), b"asset").unwrap();
        assert_eq!(
            recover(root, &anchor, &limits).unwrap().state,
            RecoveryState::Published
        );
        assert!(!stage.exists());
        assert_eq!(fs::read(root.join(resource.as_path())).unwrap(), b"asset");
    }

    #[test]
    fn recovery_retains_replaced_payloads_in_private_staging() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let limits = Limits::default();
        let manifest = ProjectPath::new("project.cirproj").unwrap();
        fs::write(root.join(manifest.as_path()), b"previous").unwrap();
        let stage = begin(root).unwrap();
        let payload = stage.join(data_name(0));
        fs::write(&payload, b"expected").unwrap();
        write_journal(
            &stage,
            &Journal {
                version: 1,
                manifest: manifest.clone(),
                before: Digest::of(b"previous"),
                after: Digest::of(b"next"),
                files: vec![JournalFile {
                    path: ProjectPath::new("asset.bin").unwrap(),
                    digest: Digest::of(b"expected"),
                    bytes: 8,
                }],
            },
        )
        .unwrap();
        fs::write(&payload, b"external").unwrap();
        let report = recover(root, &manifest, &limits).unwrap();
        assert_eq!(report.state, RecoveryState::Diverged);
        assert_eq!(fs::read(payload).unwrap(), b"external");
        assert!(stage.join(JOURNAL).exists());
    }
}
