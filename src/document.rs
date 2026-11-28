use crate::{
    DocumentEntry, Error, Extensions, FORMAT_VERSION, Limits, Result, document_extension, json,
    manifest::{check_extensions, check_token},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DOCUMENT_FORMAT: &str = "cirnet.document";

/// Shared JSON envelope. Payload semantics and all engineering geometry belong to the domain owner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub format_version: String,
    pub id: String,
    pub kind: String,
    pub schema_version: String,
    pub payload: Value,
    #[serde(default)]
    pub extensions: Extensions,
}

/// The inspected capability is container-level. It conveys no editor or execution permission.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentRead<'a> {
    Envelope(Document),
    LegacyCircuit(&'a [u8]),
    Opaque(&'a [u8]),
}

impl Document {
    pub fn new(entry: &DocumentEntry, payload: Value) -> Self {
        Self {
            format: DOCUMENT_FORMAT.into(),
            format_version: FORMAT_VERSION.into(),
            id: entry.id.clone(),
            kind: entry.kind.clone(),
            schema_version: entry.schema_version.clone(),
            payload,
            extensions: Extensions::new(),
        }
    }

    pub fn to_bytes(&self, limits: &Limits) -> Result<Vec<u8>> {
        crate::manifest::check_version(
            "document envelope",
            &self.format,
            DOCUMENT_FORMAT,
            &self.format_version,
        )?;
        check_token(&self.id, "document ID")?;
        check_token(&self.kind, "document kind")?;
        check_token(&self.schema_version, "document schema version")?;
        check_extensions(&self.extensions)?;
        if self.kind == "circuit-graph" {
            return Err(Error::Validation(
                "use original CircuitGraph JSON bytes".into(),
            ));
        }
        json::write(self, limits)
    }
}

pub fn decode_document<'a>(
    entry: &DocumentEntry,
    bytes: &'a [u8],
    limits: &Limits,
) -> Result<DocumentRead<'a>> {
    if entry.kind == "circuit-graph" {
        let (version, _) = crate::migration::inspect_legacy(bytes, limits)?;
        if entry.schema_version != version {
            return Err(Error::Validation(format!(
                "graph version differs from registry for {}",
                entry.id
            )));
        }
        return Ok(DocumentRead::LegacyCircuit(bytes));
    }
    let value: Value = json::read(bytes, limits)?;
    let (format, version) = crate::migration::header(&value)?;
    if format != DOCUMENT_FORMAT || version != FORMAT_VERSION {
        return Ok(DocumentRead::Opaque(bytes));
    }
    let document: Document = serde_json::from_value(value)?;
    check_extensions(&document.extensions)?;
    if (
        document.id.as_str(),
        document.kind.as_str(),
        document.schema_version.as_str(),
    ) != (
        entry.id.as_str(),
        entry.kind.as_str(),
        entry.schema_version.as_str(),
    ) {
        return Err(Error::Validation(format!(
            "envelope metadata differs from registry for {}",
            entry.id
        )));
    }
    if document_extension(&entry.kind).is_none() {
        return Ok(DocumentRead::Opaque(bytes));
    }
    Ok(DocumentRead::Envelope(document))
}
