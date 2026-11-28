use super::{MANIFEST_FORMAT, MANIFEST_FORMAT_VERSION, Manifest, check_version};
use crate::{Limits, Result, json};
use serde_json::Value;
/// Read a manifest, reject unsupported versions, and check its complete reference structure.
pub fn decode_manifest(bytes: &[u8], limits: &Limits) -> Result<Manifest> {
    let value: Value = json::read(bytes, limits)?;
    let (format, version) = crate::migration::header(&value)?;
    check_version(
        "project",
        format,
        MANIFEST_FORMAT,
        version,
        MANIFEST_FORMAT_VERSION,
    )?;
    let manifest: Manifest = serde_json::from_value(value)?;
    manifest.index(limits)?;
    Ok(manifest)
}
