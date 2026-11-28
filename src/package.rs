//! ZIP package 1.0. Membership and its SHA-256 inventory are verified before publication.

use crate::storage::{
    Project, open_regular, persist_new, read_bounded, resolve, sync_dir, trusted_root, write_lock,
};
use crate::{
    Digest, Error, FORMAT_VERSION, Limits, Manifest, ProjectPath, Result, decode_document,
    decode_manifest, json,
};
use crate::{
    error::{check_limit, io},
    integrity::transfer,
    manifest::check_version,
    path::unique_paths,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use tempfile::{NamedTempFile, TempDir};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const PACKAGE_FORMAT: &str = "cirnet.package";
pub const MANIFEST_ENTRY: &str = "project.cirproj";
pub const INVENTORY_ENTRY: &str = "integrity.json";

#[derive(Clone, Copy, Debug, Default)]
pub enum Compression {
    #[default]
    Stored,
    /// DEFLATE for JSONs. Native assets stay stored to avoid recompressing archive formats.
    Deflate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    format: String,
    format_version: String,
    algorithm: String,
    entries: BTreeMap<ProjectPath, Item>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    bytes: u64,
    sha256: Digest,
}

pub struct VerifiedPackage {
    directory: TempDir,
    manifest: Manifest,
    limits: Limits,
}

fn path(name: &str) -> ProjectPath {
    ProjectPath::new(name).expect("constant package path is valid")
}

fn members(manifest: &Manifest) -> Result<BTreeMap<ProjectPath, Option<&crate::DocumentEntry>>> {
    if let Some(external) = manifest.libraries.iter().find(|entry| entry.required) {
        return Err(Error::Missing(format!(
            "required external library {} must be included locally",
            external.id
        )));
    }
    Ok(std::iter::once((path(MANIFEST_ENTRY), None))
        .chain(
            manifest
                .documents
                .iter()
                .map(|entry| (entry.path.clone(), Some(entry))),
        )
        .chain(
            manifest
                .assets
                .iter()
                .map(|entry| (entry.path.clone(), None)),
        )
        .collect())
}

fn archive_error(error: zip::result::ZipError) -> Error {
    Error::Package(error.to_string())
}

fn limit_for(path: &ProjectPath, document: bool, limits: &Limits) -> u64 {
    if document || matches!(path.as_str(), MANIFEST_ENTRY | INVENTORY_ENTRY) {
        limits.max_json_bytes
    } else {
        limits.max_asset_bytes
    }
}

/// Write a deterministic ZIP to an empty seekable sink. On failure discard the sink.
pub fn export<W: Write + Seek>(
    mut sink: W,
    project: &Project,
    compression: Compression,
) -> Result<W> {
    if sink
        .seek(SeekFrom::End(0))
        .map_err(|error| io("ZIP sink", error))?
        != 0
    {
        return Err(Error::Conflict("ZIP sink is not empty".into()));
    }
    let _lock = write_lock(project.root())?;
    project.check_revision()?;
    let limits = project.limits();
    let manifest_bytes = project.manifest().to_bytes(limits)?;
    let members = members(project.manifest())?;
    check_limit(
        "ZIP members",
        (members.len() + 1) as u64,
        (limits.max_resources as u64).saturating_add(2).min(65_534),
    )?;
    let mut archive = ZipWriter::new(sink);
    let mut inventory = Inventory {
        format: PACKAGE_FORMAT.into(),
        format_version: FORMAT_VERSION.into(),
        algorithm: "sha256".into(),
        entries: BTreeMap::new(),
    };
    let mut total = 0u64;
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let deflate = stored
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    for (resource, document) in members {
        let is_manifest = resource.as_str() == MANIFEST_ENTRY;
        let mut limit = limit_for(&resource, document.is_some(), limits)
            .min(limits.max_total_bytes.saturating_sub(total));
        // The profile deliberately uses single-disk ZIP32.
        limit = limit.min(u64::from(u32::MAX) - 1);
        let options = if (document.is_some() || is_manifest)
            && matches!(compression, Compression::Deflate)
            && limits.max_compression_ratio >= 1100
        {
            deflate
        } else {
            stored
        };
        archive
            .start_file(resource.as_str(), options)
            .map_err(archive_error)?;
        let (bytes, sha256) = if let Some(entry) = document {
            let doc = read_bounded(
                open_regular(project.root(), &resource)?,
                limit,
                resource.as_str(),
            )?;
            decode_document(entry, &doc, limits)?;
            transfer(&doc[..], &mut archive, limit, resource.as_str())?
        } else if is_manifest {
            transfer(&manifest_bytes[..], &mut archive, limit, resource.as_str())?
        } else {
            transfer(
                open_regular(project.root(), &resource)?,
                &mut archive,
                limit,
                resource.as_str(),
            )?
        };
        total += bytes;
        inventory.entries.insert(resource, Item { bytes, sha256 });
    }
    let inventory = json::write(&inventory, limits)?;
    check_limit(
        "total expanded ZIP bytes",
        total.saturating_add(inventory.len() as u64),
        limits.max_total_bytes,
    )?;
    archive
        .start_file(INVENTORY_ENTRY, stored)
        .map_err(archive_error)?;
    archive
        .write_all(&inventory)
        .map_err(|error| io(INVENTORY_ENTRY, error))?;
    let mut sink = archive.finish().map_err(archive_error)?;
    let archive_bytes = sink
        .seek(SeekFrom::End(0))
        .map_err(|error| io("ZIP size", error))?;
    check_limit(
        "ZIP bytes",
        archive_bytes,
        limits.max_archive_bytes.min(u64::from(u32::MAX) - 1),
    )?;
    let directory_bytes = (46 + INVENTORY_ENTRY.len() + 46 + MANIFEST_ENTRY.len()) as u64
        + project
            .manifest()
            .documents
            .iter()
            .map(|entry| 46 + entry.path.as_str().len() as u64)
            .sum::<u64>()
        + project
            .manifest()
            .assets
            .iter()
            .map(|entry| 46 + entry.path.as_str().len() as u64)
            .sum::<u64>();
    check_limit(
        "ZIP directory bytes",
        directory_bytes,
        limits.max_directory_bytes,
    )?;
    project.check_revision()?;
    Ok(sink)
}

/// Atomically publish a new package without replacing an existing file.
pub fn export_file(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Path(path.display().to_string()))?;
    let name = ProjectPath::new(filename)?;
    if name.extension() != Some("cirpkg") {
        return Err(Error::Path(name.to_string()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = trusted_root(parent)?;
    let mut temp = NamedTempFile::new_in(&parent).map_err(|error| io(name.to_string(), error))?;
    export(temp.as_file_mut(), project, compression)?;
    temp.as_file()
        .sync_all()
        .map_err(|error| io(name.to_string(), error))?;
    persist_new(temp, &parent.join(name.as_path()))
}

fn word(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes([bytes[start], bytes[start + 1]])
}

fn long(bytes: &[u8], start: usize) -> u32 {
    u32::from_le_bytes([
        bytes[start],
        bytes[start + 1],
        bytes[start + 2],
        bytes[start + 3],
    ])
}

fn read_header(reader: &mut (impl Read + Seek), at: u64, bytes: &mut [u8]) -> Result<()> {
    reader
        .seek(SeekFrom::Start(at))
        .and_then(|_| reader.read_exact(bytes))
        .map_err(|error| io("ZIP record", error))
}

// Validate profile boundaries before the library allocates and indexes the directory.
// In particular, the reported count must describe the entire central directory.
fn check_zip_records(
    reader: &mut (impl Read + Seek),
    start: u64,
    end: u64,
    count: usize,
) -> Result<()> {
    let mut next = start;
    let mut central = [0u8; 46];
    let mut local = [0u8; 30];
    let mut central_name = [0u8; 240];
    let mut local_name = [0u8; 240];
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        if next + central.len() as u64 > end {
            return Err(Error::Package("truncated central directory entry".into()));
        }
        read_header(reader, next, &mut central)?;
        let name_len = usize::from(word(&central, 28));
        let local_start = u64::from(long(&central, 42));
        let compressed = u64::from(long(&central, 20));
        let compressed_end = local_start + local.len() as u64 + name_len as u64 + compressed;
        if &central[..4] != b"PK\x01\x02"
            || word(&central, 34) != 0
            || word(&central, 8) & !0x0806 != 0
            || !(1..=central_name.len()).contains(&name_len)
            || compressed_end > start
        {
            return Err(Error::Package("invalid central directory entry".into()));
        }
        let next_entry = next
            + central.len() as u64
            + name_len as u64
            + u64::from(word(&central, 30))
            + u64::from(word(&central, 32));
        if next_entry > end {
            return Err(Error::Package(
                "central directory entry exceeds its bounds".into(),
            ));
        }
        reader
            .read_exact(&mut central_name[..name_len])
            .map_err(|error| io("ZIP central name", error))?;
        read_header(reader, local_start, &mut local)?;
        if &local[..4] != b"PK\x03\x04"
            || usize::from(word(&local, 26)) != name_len
            || word(&local, 6) != word(&central, 8)
            || word(&local, 8) != word(&central, 10)
            || local[14..26] != central[16..28]
            || compressed_end + u64::from(word(&local, 28)) > start
        {
            return Err(Error::Package(
                "conflicting or out-of-bounds local ZIP header".into(),
            ));
        }
        reader
            .read_exact(&mut local_name[..name_len])
            .map_err(|error| io("ZIP local name", error))?;
        if local_name[..name_len] != central_name[..name_len] {
            return Err(Error::Package("local and central ZIP names differ".into()));
        }
        ranges.push((local_start, compressed_end + u64::from(word(&local, 28))));
        next = next_entry;
    }
    if next != end {
        return Err(Error::Package(
            "ZIP central directory has unreported records".into(),
        ));
    }
    ranges.sort_unstable();
    let mut local_end = 0;
    for (offset, end) in ranges {
        if offset != local_end {
            return Err(Error::Package(
                "overlapping or undeclared local ZIP data".into(),
            ));
        }
        local_end = end;
    }
    if local_end != start {
        return Err(Error::Package(
            "undeclared data before the central directory".into(),
        ));
    }
    Ok(())
}

// Limit the central directory before the ZIP library builds its in-memory index.
// Profile 1.0 accepts canonical end records with no comment, split volumes, or ZIP64.
fn preflight(mut reader: impl Read + Seek, limits: &Limits) -> Result<usize> {
    let len = reader
        .seek(SeekFrom::End(0))
        .map_err(|error| io("ZIP size", error))?;
    check_limit(
        "ZIP bytes",
        len,
        limits.max_archive_bytes.min(u64::from(u32::MAX) - 1),
    )?;
    if len < 22 {
        return Err(Error::Package("truncated ZIP end record".into()));
    }
    reader
        .seek(SeekFrom::End(-22))
        .map_err(|error| io("ZIP end record", error))?;
    let mut end = [0u8; 22];
    reader
        .read_exact(&mut end)
        .map_err(|error| io("ZIP end record", error))?;
    let entries = word(&end, 10) as usize;
    if &end[..4] != b"PK\x05\x06"
        || word(&end, 4) != 0
        || word(&end, 6) != 0
        || word(&end, 8) != word(&end, 10)
        || word(&end, 20) != 0
        || entries as u64 * 46 > u64::from(long(&end, 12))
        || u64::from(long(&end, 12)) + u64::from(long(&end, 16)) != len - 22
    {
        return Err(Error::Package(
            "expected a single-disk, comment-free ZIP32 directory".into(),
        ));
    }
    check_limit(
        "ZIP directory bytes",
        long(&end, 12).into(),
        limits.max_directory_bytes,
    )?;
    check_limit(
        "ZIP members",
        entries as u64,
        (limits.max_resources as u64).saturating_add(2).min(65_534),
    )?;
    check_zip_records(&mut reader, u64::from(long(&end, 16)), len - 22, entries)?;
    Ok(entries)
}

fn file_for(root: &Path, name: &ProjectPath) -> Result<File> {
    let destination = resolve(root, name, true)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| io(name.to_string(), error))
}

struct Archive<R> {
    zip: ZipArchive<R>,
    inventory: Inventory,
    locations: BTreeMap<ProjectPath, usize>,
}

impl<R: Read + Seek> Archive<R> {
    fn open(mut reader: R, limits: &Limits) -> Result<Self> {
        let count = preflight(&mut reader, limits)?;
        let mut zip = ZipArchive::new(reader).map_err(archive_error)?;
        if zip.len() != count
            || zip.offset() != 0
            || zip.has_overlapping_entries().map_err(archive_error)?
        {
            return Err(Error::Package(
                "duplicate or overlapping ZIP entries".into(),
            ));
        }
        let mut paths = Vec::new();
        let mut locations = BTreeMap::new();
        for index in 0..zip.len() {
            let entry = zip.by_index_raw(index).map_err(archive_error)?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| Error::Package("non-UTF-8 ZIP name".into()))?;
            let name = ProjectPath::new(name)?;
            if !entry.is_file()
                || entry.encrypted()
                || entry.unix_mode().is_some_and(|mode| {
                    !matches!(mode & 0o170000, 0 | 0o100000) || mode & 0o7000 != 0
                })
                || !matches!(
                    entry.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                )
            {
                return Err(Error::Package(format!("unsupported ZIP member {name}")));
            }
            if entry.compression() == CompressionMethod::Stored
                && entry.compressed_size() != entry.size()
            {
                return Err(Error::Integrity(format!(
                    "inconsistent stored size for {name}"
                )));
            }
            let ratio_bound = entry
                .compressed_size()
                .saturating_mul(limits.max_compression_ratio);
            check_limit("ZIP expansion ratio", entry.size(), ratio_bound)?;
            check_limit(
                "ZIP member bytes",
                entry.size(),
                limits.max_json_bytes.max(limits.max_asset_bytes),
            )?;
            paths.push(name.clone());
            locations.insert(name, index);
        }
        unique_paths(&paths)?;
        let index = *locations
            .get(&path(INVENTORY_ENTRY))
            .ok_or_else(|| Error::Missing(INVENTORY_ENTRY.into()))?;
        let inventory_size = zip.by_index_raw(index).map_err(archive_error)?.size();
        check_limit(
            "inventory bytes",
            inventory_size,
            limits.max_json_bytes.min(limits.max_total_bytes),
        )?;
        let inventory_bytes = read_bounded(
            zip.by_index(index).map_err(archive_error)?,
            inventory_size,
            INVENTORY_ENTRY,
        )?;
        if inventory_bytes.len() as u64 != inventory_size {
            return Err(Error::Integrity("inventory byte count".into()));
        }
        let inventory: Inventory = json::read(&inventory_bytes, limits)?;
        check_version(
            "package",
            &inventory.format,
            PACKAGE_FORMAT,
            &inventory.format_version,
        )?;
        if inventory.algorithm != "sha256" {
            return Err(Error::Integrity("unsupported inventory algorithm".into()));
        }
        if inventory.entries.len() != paths.len().saturating_sub(1)
            || inventory.entries.contains_key(&path(INVENTORY_ENTRY))
            || inventory
                .entries
                .keys()
                .any(|name| !locations.contains_key(name))
        {
            return Err(Error::Integrity(
                "inventory membership differs from archive".into(),
            ));
        }
        let mut total = inventory_size;
        for (name, expected) in &inventory.entries {
            let entry = zip.by_index_raw(locations[name]).map_err(archive_error)?;
            if entry.size() != expected.bytes {
                return Err(Error::Integrity(format!("size of {name}")));
            }
            total = total.saturating_add(expected.bytes);
            check_limit("total expanded ZIP bytes", total, limits.max_total_bytes)?;
        }
        Ok(Self {
            zip,
            inventory,
            locations,
        })
    }

    fn read(&mut self, name: &ProjectPath, writer: impl Write, limit: u64) -> Result<()> {
        let expected = self
            .inventory
            .entries
            .get(name)
            .ok_or_else(|| Error::Missing(name.to_string()))?;
        check_limit("declared ZIP member bytes", expected.bytes, limit)?;
        let entry = self
            .zip
            .by_index(self.locations[name])
            .map_err(archive_error)?;
        let (bytes, sha256) = transfer(entry, writer, expected.bytes, name.as_str())?;
        if bytes != expected.bytes || sha256 != expected.sha256 {
            return Err(Error::Integrity(name.to_string()));
        }
        Ok(())
    }
}

/// Inspect the verified manifest and declared membership without reading asset payloads.
/// Use verify before trusting that the remaining bytes match the inventory.
pub fn inspect<R: Read + Seek>(reader: R, limits: Limits) -> Result<Manifest> {
    let mut archive = Archive::open(reader, &limits)?;
    inspect_manifest(&mut archive, &limits).map(|(manifest, _)| manifest)
}

fn inspect_manifest<R: Read + Seek>(
    archive: &mut Archive<R>,
    limits: &Limits,
) -> Result<(Manifest, Vec<u8>)> {
    let manifest_name = path(MANIFEST_ENTRY);
    let mut manifest_bytes = Vec::new();
    archive.read(&manifest_name, &mut manifest_bytes, limits.max_json_bytes)?;
    let manifest = decode_manifest(&manifest_bytes, limits)?;
    let members = members(&manifest)?;
    if !members.keys().eq(archive.inventory.entries.keys()) {
        return Err(Error::Integrity(
            "manifest membership differs from inventory".into(),
        ));
    }
    for (name, doc) in members {
        check_limit(
            "declared ZIP member bytes",
            archive.inventory.entries[&name].bytes,
            limit_for(&name, doc.is_some(), limits),
        )?;
    }
    Ok((manifest, manifest_bytes))
}

/// Validate every member into private scratch storage. No import destination is opened here.
/// The scratch parent must be a local directory controlled by the caller.
pub fn verify<R: Read + Seek>(
    reader: R,
    scratch_parent: impl AsRef<Path>,
    limits: Limits,
) -> Result<VerifiedPackage> {
    let mut archive = Archive::open(reader, &limits)?;
    let manifest_name = path(MANIFEST_ENTRY);
    let (manifest, manifest_bytes) = inspect_manifest(&mut archive, &limits)?;
    let members = members(&manifest)?;
    let directory = tempfile::Builder::new()
        .prefix(".cirnet-verify-")
        .tempdir_in(trusted_root(scratch_parent.as_ref())?)
        .map_err(|error| io("private package stage", error))?;
    for (name, doc) in members {
        let limit = limit_for(&name, doc.is_some(), &limits);
        let mut file = file_for(directory.path(), &name)?;
        if name == manifest_name {
            file.write_all(&manifest_bytes)
                .map_err(|error| io(MANIFEST_ENTRY, error))?;
        } else {
            archive.read(&name, &mut file, limit)?;
        }
        file.sync_all()
            .map_err(|error| io(name.to_string(), error))?;
        if let Some(entry) = doc {
            let bytes = read_bounded(open_regular(directory.path(), &name)?, limit, name.as_str())?;
            decode_document(entry, &bytes, &limits)?;
        }
    }
    Ok(VerifiedPackage {
        directory,
        manifest,
        limits,
    })
}

impl VerifiedPackage {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Publish to a new directory only. An interruption before the final replacement leaves no manifest.
    /// A destination on another volume is supported by bounded copies.
    pub fn publish(self, destination: impl AsRef<Path>) -> Result<Project> {
        let destination = destination.as_ref();
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::Path(destination.display().to_string()))?;
        let name = ProjectPath::new(name)?;
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let root = trusted_root(parent)?.join(name.as_path());
        fs::create_dir(&root).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Conflict(root.display().to_string())
            } else {
                io(root.display().to_string(), error)
            }
        })?;
        sync_dir(root.parent().expect("resolved root has parent"))?;
        let manifest_name = path(MANIFEST_ENTRY);
        let members = members(&self.manifest)?;
        for (name, doc) in members {
            if name == manifest_name {
                continue;
            }
            let target = resolve(&root, &name, true)?;
            let mut temp = NamedTempFile::new_in(target.parent().expect("target has parent"))
                .map_err(|error| io(name.to_string(), error))?;
            transfer(
                open_regular(self.directory.path(), &name)?,
                temp.as_file_mut(),
                limit_for(&name, doc.is_some(), &self.limits),
                name.as_str(),
            )?;
            temp.as_file()
                .sync_all()
                .map_err(|error| io(name.to_string(), error))?;
            persist_new(temp, &target)?;
        }
        let bytes = read_bounded(
            open_regular(self.directory.path(), &manifest_name)?,
            self.limits.max_json_bytes,
            MANIFEST_ENTRY,
        )?;
        let file = crate::storage::temp_bytes(&root, &bytes)?;
        let target = root.join(manifest_name.as_path());
        persist_new(file, &target)?;
        Ok(Project::from_parts(
            root,
            manifest_name,
            self.manifest,
            Digest::of(&bytes),
            self.limits,
        ))
    }
}
