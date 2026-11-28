use super::{
    INVENTORY_ENTRY, Inventory, MANIFEST_ENTRY, Operation, PACKAGE_FORMAT, PACKAGE_VERSION,
    archive_error, limit_for, members, path, read_bounded,
};
use crate::{
    Error, Limits, Manifest, ProjectPath, Result, decode_document, decode_manifest,
    error::{check_limit, io},
    integrity::transfer_with,
    json,
    manifest::check_version,
    path::unique_paths,
};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
};
use zip::{CompressionMethod, ZipArchive};

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
    operation: &mut Operation<'_>,
) -> Result<()> {
    let mut next = start;
    let mut central = [0u8; 46];
    let mut local = [0u8; 30];
    let mut central_name = [0u8; 240];
    let mut local_name = [0u8; 240];
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        operation.check(None, 0)?;
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
fn preflight(
    mut reader: impl Read + Seek,
    limits: &Limits,
    operation: &mut Operation<'_>,
) -> Result<usize> {
    operation.check(None, 0)?;
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
    check_zip_records(
        &mut reader,
        u64::from(long(&end, 16)),
        len - 22,
        entries,
        operation,
    )?;
    Ok(entries)
}

struct Archive<R> {
    zip: ZipArchive<R>,
    inventory: Inventory,
    locations: BTreeMap<ProjectPath, usize>,
}

impl<R: Read + Seek> Archive<R> {
    fn open(mut reader: R, limits: &Limits, operation: &mut Operation<'_>) -> Result<Self> {
        let count = preflight(&mut reader, limits, operation)?;
        operation.check(None, 0)?;
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
            operation.check(None, 0)?;
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
        operation.check(Some(INVENTORY_ENTRY), 0)?;
        let inventory_bytes = read_bounded(
            zip.by_index(index).map_err(archive_error)?,
            inventory_size,
            INVENTORY_ENTRY,
            |bytes| operation.check(Some(INVENTORY_ENTRY), bytes),
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
            PACKAGE_VERSION,
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
            operation.check(None, 0)?;
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

    fn read(
        &mut self,
        name: &ProjectPath,
        writer: impl Write,
        limit: u64,
        operation: &mut Operation<'_>,
    ) -> Result<()> {
        operation.check(Some(name.as_str()), 0)?;
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
        let (bytes, sha256) =
            transfer_with(entry, writer, expected.bytes, name.as_str(), |bytes| {
                operation.check(Some(name.as_str()), bytes)
            })?;
        if bytes != expected.bytes || sha256 != expected.sha256 {
            return Err(Error::Integrity(name.to_string()));
        }
        Ok(())
    }
}

pub(super) struct Inspected<R> {
    archive: Archive<R>,
    pub(super) manifest: Manifest,
    manifest_bytes: Vec<u8>,
}

impl<R: Read + Seek> Inspected<R> {
    pub(super) fn open(reader: R, limits: &Limits, operation: &mut Operation<'_>) -> Result<Self> {
        let mut archive = Archive::open(reader, limits, operation)?;
        let manifest_name = path(MANIFEST_ENTRY);
        let mut manifest_bytes = Vec::new();
        archive.read(
            &manifest_name,
            &mut manifest_bytes,
            limits.max_json_bytes,
            operation,
        )?;
        let manifest = decode_manifest(&manifest_bytes, limits)?;
        let members = members(&manifest)?;
        if !members.keys().eq(archive.inventory.entries.keys()) {
            return Err(Error::Integrity(
                "manifest membership differs from inventory".into(),
            ));
        }
        for (name, doc) in members {
            operation.check(None, 0)?;
            check_limit(
                "declared ZIP member bytes",
                archive.inventory.entries[&name].bytes,
                limit_for(&name, doc.is_some(), limits),
            )?;
        }
        Ok(Self {
            archive,
            manifest,
            manifest_bytes,
        })
    }

    pub(super) fn verify(
        self,
        limits: &Limits,
        stage: &mut impl Stage,
        operation: &mut Operation<'_>,
    ) -> Result<(Manifest, Inventory)> {
        let Self {
            mut archive,
            manifest,
            manifest_bytes,
        } = self;
        let manifest_name = path(MANIFEST_ENTRY);
        for (name, doc) in members(&manifest)? {
            let limit = limit_for(&name, doc.is_some(), limits);
            if name == manifest_name {
                stage.store(&name, &manifest_bytes)?;
            } else if let Some(entry) = doc {
                let mut bytes = Vec::new();
                archive.read(&name, &mut bytes, limit, operation)?;
                decode_document(entry, &bytes, limits)?;
                stage.store(&name, &bytes)?;
            } else {
                let mut writer = stage.create(&name)?;
                archive.read(&name, &mut writer, limit, operation)?;
                stage.finish(&name, writer)?;
            }
        }
        operation.check(None, 0)?;
        Ok((manifest, archive.inventory))
    }
}

pub(super) trait Stage {
    type Writer: Write;

    fn create(&mut self, name: &ProjectPath) -> Result<Self::Writer>;
    fn finish(&mut self, name: &ProjectPath, writer: Self::Writer) -> Result<()>;

    fn store(&mut self, name: &ProjectPath, bytes: &[u8]) -> Result<()> {
        let mut writer = self.create(name)?;
        writer
            .write_all(bytes)
            .map_err(|error| io(name.as_str(), error))?;
        self.finish(name, writer)
    }
}

pub(super) struct Discard;

impl Stage for Discard {
    type Writer = std::io::Sink;

    fn create(&mut self, _: &ProjectPath) -> Result<Self::Writer> {
        Ok(std::io::sink())
    }

    fn finish(&mut self, _: &ProjectPath, _: Self::Writer) -> Result<()> {
        Ok(())
    }
}
