use super::{
    Compression, INVENTORY_ENTRY, Inventory, Item, MANIFEST_ENTRY, Operation, PACKAGE_FORMAT,
    PACKAGE_VERSION, PackageSource, archive_error, limit_for, members, read_bounded,
};
use crate::{
    Error, Limits, Manifest, Result, decode_document,
    error::{check_limit, io},
    integrity::transfer_with,
    json,
};
use std::io::{Seek, SeekFrom, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub(super) fn check_empty_sink(sink: &mut impl Seek) -> Result<()> {
    if sink
        .seek(SeekFrom::End(0))
        .map_err(|error| io("ZIP sink", error))?
        != 0
    {
        return Err(Error::Conflict("ZIP sink is not empty".into()));
    }
    Ok(())
}

pub(super) fn export_from<W: Write + Seek>(
    mut sink: W,
    manifest: &Manifest,
    source: &(impl PackageSource + ?Sized),
    compression: Compression,
    limits: &Limits,
    operation: &mut Operation<'_>,
) -> Result<W> {
    check_empty_sink(&mut sink)?;
    export_into_empty(sink, manifest, source, compression, limits, operation)
}

pub(super) fn export_into_empty<W: Write + Seek>(
    sink: W,
    manifest: &Manifest,
    source: &(impl PackageSource + ?Sized),
    compression: Compression,
    limits: &Limits,
    operation: &mut Operation<'_>,
) -> Result<W> {
    operation.check(None, 0)?;
    let manifest_bytes = manifest.to_bytes(limits)?;
    let members = members(manifest)?;
    check_limit(
        "ZIP members",
        (members.len() + 1) as u64,
        (limits.max_resources as u64).saturating_add(2).min(65_534),
    )?;
    let mut archive = ZipWriter::new(sink);
    let mut inventory = Inventory {
        format: PACKAGE_FORMAT.into(),
        format_version: PACKAGE_VERSION.into(),
        algorithm: "sha256".into(),
        entries: Default::default(),
    };
    let mut total = 0u64;
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let deflate = stored
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    for (resource, document) in members {
        operation.check(Some(resource.as_str()), 0)?;
        let is_manifest = resource.as_str() == MANIFEST_ENTRY;
        // The profile deliberately uses single-disk ZIP32.
        let limit = limit_for(&resource, document.is_some(), limits)
            .min(limits.max_total_bytes.saturating_sub(total))
            .min(u64::from(u32::MAX) - 1);
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
        let (document_bytes, mut bytes, mut stream);
        let reader: &mut dyn std::io::Read = if is_manifest {
            bytes = manifest_bytes.as_slice();
            &mut bytes
        } else {
            stream = source.open(&resource)?;
            if let Some(entry) = document {
                document_bytes = read_bounded(&mut stream, limit, resource.as_str(), |_| {
                    operation.check(None, 0)
                })?;
                decode_document(entry, &document_bytes, limits)?;
                // Use the exact authored bytes, including opaque document versions.
                bytes = document_bytes.as_slice();
                &mut bytes
            } else {
                &mut stream
            }
        };
        let (bytes, sha256) =
            transfer_with(reader, &mut archive, limit, resource.as_str(), |bytes| {
                operation.check(Some(resource.as_str()), bytes)
            })?;
        total += bytes;
        inventory.entries.insert(resource, Item { bytes, sha256 });
    }
    let inventory = json::write(&inventory, limits)?;
    check_limit(
        "total expanded ZIP bytes",
        total.saturating_add(inventory.len() as u64),
        limits.max_total_bytes,
    )?;
    operation.check(Some(INVENTORY_ENTRY), 0)?;
    archive
        .start_file(INVENTORY_ENTRY, stored)
        .map_err(archive_error)?;
    let mut inventory_bytes = 0;
    for chunk in inventory.chunks(64 * 1024) {
        archive
            .write_all(chunk)
            .map_err(|error| io(INVENTORY_ENTRY, error))?;
        inventory_bytes += chunk.len() as u64;
        operation.check(Some(INVENTORY_ENTRY), inventory_bytes)?;
    }
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
        + manifest
            .documents
            .iter()
            .map(|entry| 46 + entry.path.as_str().len() as u64)
            .sum::<u64>()
        + manifest
            .assets
            .iter()
            .map(|entry| 46 + entry.path.as_str().len() as u64)
            .sum::<u64>();
    check_limit(
        "ZIP directory bytes",
        directory_bytes,
        limits.max_directory_bytes,
    )?;
    operation.check(None, 0)?;
    Ok(sink)
}
