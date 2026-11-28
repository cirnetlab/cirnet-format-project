use crate::{Error, Limits, Result, json};
use serde_json::Value;
pub(crate) fn header(value: &Value) -> Result<(&str, &str)> {
    let format = value.get("format").and_then(Value::as_str);
    let version = value.get("format_version").and_then(Value::as_str);
    format
        .zip(version)
        .ok_or_else(|| Error::Validation("expected string format and format_version fields".into()))
}

/// Detect a format and its independent container version without selecting a decoder.
pub fn detect_version(bytes: &[u8], limits: &Limits) -> Result<(String, String)> {
    let value: Value = json::read(bytes, limits)?;
    let (format, version) = header(&value)?;
    Ok((format.into(), version.into()))
}
