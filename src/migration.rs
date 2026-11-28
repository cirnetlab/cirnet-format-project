use crate::{DocumentEntry, Error, Extensions, Limits, Manifest, ProjectPath, Result, json};
use serde_json::Value;
use std::collections::HashSet;

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

pub(crate) fn inspect_legacy(bytes: &[u8], limits: &Limits) -> Result<(String, String)> {
    let value: Value = json::read(bytes, limits)?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Validation("CircuitGraph schema_version is required".into()))?;
    if !matches!(version, "1.0" | "1.1" | "1.2") {
        return Err(Error::Unsupported {
            domain: "CircuitGraph",
            version: version.into(),
        });
    }
    let id = value
        .get("circuit_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    if !value.get("modules").is_some_and(Value::is_array)
        || !value.get("connections").is_some_and(Value::is_array)
    {
        return Err(Error::Validation(
            "CircuitGraph requires modules and connections arrays".into(),
        ));
    }
    let id = id.ok_or_else(|| Error::Validation("CircuitGraph circuit_id is required".into()))?;
    Ok((version.into(), id.into()))
}

/// Import only after the consumer validates with the owner-published Contracts validator.
/// The returned resource borrows the input bytes, including unknown owner metadata and precision.
pub fn import_legacy_circuit<'a>(
    bytes: &'a [u8],
    project_id: &str,
    project_name: &str,
    document_id: &str,
    path: ProjectPath,
    limits: &Limits,
    validate_contract: impl FnOnce(&str, &[u8]) -> std::result::Result<(), String>,
) -> Result<(Manifest, &'a [u8])> {
    let (version, _) = inspect_legacy(bytes, limits)?;
    validate_contract(&version, bytes).map_err(|reason| {
        Error::Validation(format!("Contracts rejected CircuitGraph: {reason}"))
    })?;
    let mut manifest = Manifest::new(project_id, project_name, vec!["electronics".into()]);
    manifest.documents.push(DocumentEntry {
        id: document_id.into(),
        title: None,
        kind: "circuit-graph".into(),
        schema_version: version,
        path,
        links: vec![],
        extensions: Extensions::new(),
    });
    manifest.index(limits)?;
    Ok((manifest, bytes))
}
