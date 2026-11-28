use super::header;
use crate::{Error, Limits, Result, json};
use serde_json::Value;
use std::collections::HashSet;
/// Explicit owner-provided transition. The core ships no hypothetical historic migrations.
pub struct Migration {
    pub from: &'static str,
    pub to: &'static str,
    pub apply: fn(Value) -> Result<Value>,
}

pub fn migrate_json(
    bytes: &[u8],
    format: &str,
    target: &str,
    steps: &[Migration],
    limits: &Limits,
) -> Result<Vec<u8>> {
    let mut value: Value = json::read(bytes, limits)?;
    let mut visited = HashSet::new();
    loop {
        let (actual, version) = header(&value)?;
        if actual != format {
            return Err(Error::Unsupported {
                domain: "migration format",
                version: actual.into(),
            });
        }
        if version == target {
            return json::write(&value, limits);
        }
        if !visited.insert(version.to_owned()) {
            return Err(Error::Validation("migration cycle".into()));
        }
        let mut available = steps.iter().filter(|step| step.from == version);
        let step = available.next().ok_or_else(|| Error::Unsupported {
            domain: "migration source",
            version: version.into(),
        })?;
        if available.next().is_some() {
            return Err(Error::Validation("ambiguous migration transition".into()));
        }
        value = (step.apply)(value)?;
        if header(&value)? != (format, step.to) {
            return Err(Error::Validation(
                "migration returned an unexpected format or version".into(),
            ));
        }
    }
}
