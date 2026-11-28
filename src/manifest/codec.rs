use super::{MANIFEST_FORMAT, MANIFEST_FORMAT_VERSION, Manifest, check_version};
use crate::{Error, Limits, Result, json};
use serde_json::Value;

/// Read a manifest, reject unsupported versions, and check its complete reference structure.
pub fn decode_manifest(bytes: &[u8], limits: &Limits) -> Result<Manifest> {
    json::preflight(bytes, limits)?;
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(Error::Validation(
            "expected string format and format_version fields".into(),
        ));
    }
    let manifest: Manifest = match serde_json::from_slice(bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            // An unsupported future manifest can also have unknown fields or missing
            // current fields. Preserve version diagnostics after a failed typed decode.
            let value: Value = serde_json::from_slice(bytes)?;
            let (format, version) = crate::migration::header(&value)?;
            check_version(
                "project",
                format,
                MANIFEST_FORMAT,
                version,
                MANIFEST_FORMAT_VERSION,
            )?;
            return Err(error.into());
        }
    };
    manifest.index(limits)?;
    Ok(manifest)
}
