//! ZIP package 1.0. Portable readers and writers use seekable streams.
//! Native verification can stage a package before publishing it to disk.

mod read;
mod write;

#[cfg(feature = "native")]
mod native;

#[cfg(feature = "native")]
pub use native::{
    VerifiedPackage, export, export_file, export_file_with_progress, export_with_progress, verify,
    verify_with_progress,
};

use crate::{Digest, Error, Limits, Manifest, ProjectPath, Result, error::io};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, Write},
    ops::ControlFlow,
};

pub const PACKAGE_FORMAT: &str = "cirnet.package";
pub const PACKAGE_VERSION: &str = "1.0";
pub const MANIFEST_ENTRY: &str = "project.cirproj";
pub const INVENTORY_ENTRY: &str = "integrity.json";

#[derive(Clone, Copy, Debug, Default)]
pub enum Compression {
    #[default]
    Stored,
    /// DEFLATE for JSONs. Assets stay stored to avoid recompressing archive formats.
    Deflate,
}

/// Opens the registered resources in a manifest. The manifest itself is supplied separately.
///
/// A source must provide a stable view for the duration of export. A resource reader starts
/// at the first byte and reads to EOF. Each registered document and asset is opened once.
pub trait PackageSource {
    type Reader<'a>: Read
    where
        Self: 'a;

    fn open(&self, path: &ProjectPath) -> Result<Self::Reader<'_>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackagePhase {
    Inspect,
    Verify,
    Export,
    Publish,
}

/// A cooperative cancellation checkpoint. Return [ControlFlow::Break] to stop with [Error::Cancelled].
///
/// Bytes count from the beginning of the named member. A checkpoint without a member
/// covers operation setup, metadata, or finalization. The callback runs synchronously.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageProgress<'a> {
    pub phase: PackagePhase,
    pub member: Option<&'a str>,
    pub bytes: u64,
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

type ProgressCallback<'a> = dyn FnMut(PackageProgress<'_>) -> ControlFlow<()> + 'a;

struct Operation<'a> {
    phase: PackagePhase,
    callback: Option<&'a mut ProgressCallback<'a>>,
}

impl<'a> Operation<'a> {
    fn new(phase: PackagePhase, callback: &'a mut ProgressCallback<'a>) -> Self {
        Self {
            phase,
            callback: Some(callback),
        }
    }

    fn quiet(phase: PackagePhase) -> Self {
        Self {
            phase,
            callback: None,
        }
    }

    fn check(&mut self, member: Option<&str>, bytes: u64) -> Result<()> {
        if let Some(callback) = &mut self.callback
            && callback(PackageProgress {
                phase: self.phase,
                member,
                bytes,
            })
            .is_break()
        {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

/// Inspect the verified manifest and declared membership without reading asset payloads.
/// Use [verify_only] to validate the rest of the package.
pub fn inspect<R: Read + Seek>(reader: R, limits: Limits) -> Result<Manifest> {
    inspect_with(reader, limits, &mut Operation::quiet(PackagePhase::Inspect))
}

pub fn inspect_with_progress<R: Read + Seek>(
    reader: R,
    limits: Limits,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<Manifest> {
    inspect_with(
        reader,
        limits,
        &mut Operation::new(PackagePhase::Inspect, &mut progress),
    )
}

fn inspect_with<R: Read + Seek>(
    reader: R,
    limits: Limits,
    operation: &mut Operation<'_>,
) -> Result<Manifest> {
    read::Inspected::open(reader, &limits, operation).map(|inspected| inspected.manifest)
}

/// Verify all member bytes and document envelopes using bounded streaming and no filesystem.
pub fn verify_only<R: Read + Seek>(reader: R, limits: Limits) -> Result<Manifest> {
    verify_only_with(reader, limits, &mut Operation::quiet(PackagePhase::Verify))
}

pub fn verify_only_with_progress<R: Read + Seek>(
    reader: R,
    limits: Limits,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<Manifest> {
    verify_only_with(
        reader,
        limits,
        &mut Operation::new(PackagePhase::Verify, &mut progress),
    )
}

fn verify_only_with<R: Read + Seek>(
    reader: R,
    limits: Limits,
    operation: &mut Operation<'_>,
) -> Result<Manifest> {
    read::Inspected::open(reader, &limits, operation)?.verify(
        &limits,
        &mut read::Discard,
        operation,
    )
}

/// Write a deterministic ZIP to an empty seekable sink. On failure discard the sink.
pub fn export_from<W: Write + Seek>(
    sink: W,
    manifest: &Manifest,
    source: &(impl PackageSource + ?Sized),
    compression: Compression,
    limits: Limits,
) -> Result<W> {
    write::export_from(
        sink,
        manifest,
        source,
        compression,
        &limits,
        &mut Operation::quiet(PackagePhase::Export),
    )
}

/// Export using cooperative progress. Discard the sink on cancellation.
pub fn export_from_with_progress<W: Write + Seek>(
    sink: W,
    manifest: &Manifest,
    source: &(impl PackageSource + ?Sized),
    compression: Compression,
    limits: Limits,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<W> {
    write::export_from(
        sink,
        manifest,
        source,
        compression,
        &limits,
        &mut Operation::new(PackagePhase::Export, &mut progress),
    )
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

fn read_bounded(
    reader: impl Read,
    limit: u64,
    context: &str,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut reader = reader.take(limit.saturating_add(1));
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(|error| io(context, error))?,
        };
        if count == 0 {
            return Ok(bytes);
        }
        crate::error::check_limit("file bytes", bytes.len() as u64 + count as u64, limit)?;
        bytes.extend_from_slice(&buffer[..count]);
        progress(bytes.len() as u64)?;
    }
}
