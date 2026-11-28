//! Portable document structure, exact-byte preservation and owner codec registration.

use crate::{
    DocumentEntry, Error, Extensions, Limits, Result, json,
    manifest::{check_extensions, check_token, check_version},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod registry;
pub use registry::{
    CodecKey, CodecRegistration, CodecRegistry, DocumentCapabilities, DocumentInspection,
    DocumentMigration, MigrationValidator,
};

pub const DOCUMENT_FORMAT: &str = "cirnet.document";
pub const DOCUMENT_ENVELOPE_VERSION: &str = "1.0";
pub const DOCUMENT_ENVELOPE_FORMAT_VERSION: &str = DOCUMENT_ENVELOPE_VERSION;

/// The portable format's known document kinds and owner-defined kinds.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DocumentKind {
    Schematic,
    Pcb,
    Assembly,
    Library,
    CircuitGraph,
    Other(String),
}

impl DocumentKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Schematic => "schematic",
            Self::Pcb => "pcb",
            Self::Assembly => "assembly",
            Self::Library => "library",
            Self::CircuitGraph => "circuit-graph",
            Self::Other(kind) => kind,
        }
    }

    /// The kind's reserved filename extension, if the portable format defines one.
    pub fn extension(&self) -> Option<&'static str> {
        Some(match self {
            Self::Schematic => "cirsch",
            Self::Pcb => "cirpcb",
            Self::Assembly => "cir3d",
            Self::Library => "cirlib",
            Self::CircuitGraph => "json",
            Self::Other(_) => return None,
        })
    }

    fn built_in(kind: &str) -> Option<Self> {
        Some(match kind {
            "schematic" => Self::Schematic,
            "pcb" => Self::Pcb,
            "assembly" => Self::Assembly,
            "library" => Self::Library,
            "circuit-graph" => Self::CircuitGraph,
            _ => return None,
        })
    }
}

impl From<&str> for DocumentKind {
    fn from(kind: &str) -> Self {
        Self::built_in(kind).unwrap_or_else(|| Self::Other(kind.into()))
    }
}

/// Supported portable envelope representations. Legacy CircuitGraph has no outer envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnvelopeVersion {
    V1,
    LegacyCircuitGraph,
}

/// Returns the reserved filename extension of a built-in document kind.
pub fn document_extension(kind: &str) -> Option<&'static str> {
    DocumentKind::built_in(kind).and_then(|kind| kind.extension())
}

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
            format_version: DOCUMENT_ENVELOPE_VERSION.into(),
            id: entry.id.clone(),
            kind: entry.kind.clone(),
            schema_version: entry.schema_version.clone(),
            payload,
            extensions: Extensions::new(),
        }
    }

    pub fn to_bytes(&self, limits: &Limits) -> Result<Vec<u8>> {
        check_version(
            "document envelope",
            &self.format,
            DOCUMENT_FORMAT,
            &self.format_version,
            DOCUMENT_ENVELOPE_VERSION,
        )?;
        check_token(&self.id, "document ID")?;
        check_token(&self.kind, "document kind")?;
        check_token(&self.schema_version, "document schema version")?;
        check_extensions(&self.extensions)?;
        if self.kind == DocumentKind::CircuitGraph.as_str() {
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
    let document = read_structure(entry, bytes, limits)?;
    if matches!(document, DocumentRead::Envelope(_))
        && DocumentKind::built_in(&entry.kind).is_none()
    {
        return Ok(DocumentRead::Opaque(bytes));
    }
    Ok(document)
}

fn read_structure<'a>(
    entry: &DocumentEntry,
    bytes: &'a [u8],
    limits: &Limits,
) -> Result<DocumentRead<'a>> {
    if entry.kind == DocumentKind::CircuitGraph.as_str() {
        let version = crate::migration::inspect_legacy(bytes, limits)?;
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
    if format != DOCUMENT_FORMAT || version != DOCUMENT_ENVELOPE_VERSION {
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
    Ok(DocumentRead::Envelope(document))
}

/// Check preservation for an already decoded change. Callers validate identity, revision and
/// domain authorization separately. Unchanged opaque bytes can be carried through a transaction.
pub fn validate_document_transition(
    entry: &DocumentEntry,
    before: &DocumentRead<'_>,
    after: &DocumentRead<'_>,
) -> Result<()> {
    match (before, after) {
        (DocumentRead::Envelope(old), DocumentRead::Envelope(new)) => {
            if old
                .extensions
                .iter()
                .all(|(key, value)| new.extensions.get(key) == Some(value))
            {
                Ok(())
            } else {
                Err(Error::Validation(
                    "document extension data would be changed or lost".into(),
                ))
            }
        }
        (DocumentRead::LegacyCircuit(_), DocumentRead::LegacyCircuit(_)) => Ok(()),
        (DocumentRead::Opaque(old), DocumentRead::Opaque(new)) if old == new => Ok(()),
        _ => Err(Error::Unsupported {
            domain: "document write",
            version: entry.schema_version.clone(),
        }),
    }
}
