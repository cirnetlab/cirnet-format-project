use crate::{DocumentEntry, Error, Extensions, Limits, Manifest, ProjectPath, Result, json};
use serde_json::Value;
pub(crate) fn inspect_legacy(bytes: &[u8], limits: &Limits) -> Result<String> {
    let value: Value = json::read(bytes, limits)?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Validation("CircuitGraph schema_version is required".into()))?;
    check_legacy_version(version)?;
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
    if id.is_none() {
        return Err(Error::Validation(
            "CircuitGraph circuit_id is required".into(),
        ));
    }
    Ok(version.into())
}

pub(crate) fn check_legacy_version(version: &str) -> Result<()> {
    if matches!(version, "1.0" | "1.1" | "1.2") {
        return Ok(());
    }
    Err(Error::Unsupported {
        domain: "CircuitGraph",
        version: version.into(),
    })
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
    let version = inspect_legacy(bytes, limits)?;
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
