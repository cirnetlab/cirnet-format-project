use super::Extensions;
use crate::{Error, Result};
pub(super) fn is_media_part(value: &str) -> bool {
    (1..=127).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&byte))
}

pub(crate) fn is_token(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-'))
}

pub(crate) fn check_token(value: &str, label: &str) -> Result<()> {
    if !is_token(value) {
        return Err(Error::Validation(format!("invalid {label}")));
    }
    Ok(())
}

pub(crate) fn check_text(value: &str, label: &str, length: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > length
        || value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        return Err(Error::Validation(format!("invalid {label}")));
    }
    Ok(())
}

pub(crate) fn check_extensions(value: &Extensions) -> Result<()> {
    for key in value.keys() {
        if !is_token(key) || !key.split('.').all(is_token) || !key.contains('.') {
            return Err(Error::Validation(
                "expected dotted namespaces for extension data".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_version(
    domain: &'static str,
    format: &str,
    expected: &str,
    version: &str,
    expected_version: &str,
) -> Result<()> {
    if format != expected || version != expected_version {
        let version = if format == expected {
            version.to_owned()
        } else {
            format!("{format}/{version}")
        };
        return Err(Error::Unsupported { domain, version });
    }
    Ok(())
}
