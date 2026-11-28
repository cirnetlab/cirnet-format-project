use super::{DocumentKind, DocumentRead, EnvelopeVersion, read_structure};
use crate::{DocumentEntry, Error, Limits, Result, manifest::check_token};
use std::collections::{HashMap, hash_map::Entry};

/// Codec selection uses the kind, the envelope representation and the exact owner schema.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CodecKey {
    pub kind: DocumentKind,
    pub envelope_version: EnvelopeVersion,
    pub schema_version: String,
}

/// An explicit owner registration. The validator receives the original bytes of the document.
#[derive(Clone, Debug)]
pub struct CodecRegistration {
    pub key: CodecKey,
    pub validate: fn(&DocumentEntry, &[u8]) -> Result<()>,
    pub writable: bool,
    pub migratable: bool,
}

/// Capabilities describe what the core and a registered owner codec can do with this document.
/// Domain support indicates an available validator. Call the validator before acting on content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentCapabilities {
    pub structural_read: bool,
    pub domain_support: bool,
    pub byte_preservation: bool,
    pub writable: bool,
    pub migratable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentInspection<'a> {
    pub document: DocumentRead<'a>,
    pub capabilities: DocumentCapabilities,
    original: &'a [u8],
}

impl<'a> DocumentInspection<'a> {
    /// Returns the exact input, including whitespace, field order and numeric spellings.
    pub fn original_bytes(&self) -> &'a [u8] {
        self.original
    }
}

/// Owner codecs supplied by the host. An empty registry supports portable structure and
/// opaque preservation, with no domain write or migration capabilities.
#[derive(Clone, Debug, Default)]
pub struct CodecRegistry {
    registrations: HashMap<CodecKey, CodecRegistration>,
}

impl CodecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a codec for one exact key. Repeated and inconsistent registrations are errors.
    pub fn register(&mut self, registration: CodecRegistration) -> Result<()> {
        let key = &registration.key;
        check_token(key.kind.as_str(), "document kind")?;
        check_token(&key.schema_version, "document schema version")?;
        if key.kind != DocumentKind::from(key.kind.as_str()) {
            return Err(Error::Validation(
                "document kind must use its built-in variant".into(),
            ));
        }
        let legacy = key.kind == DocumentKind::CircuitGraph;
        if legacy != (key.envelope_version == EnvelopeVersion::LegacyCircuitGraph) {
            return Err(Error::Validation(
                "document kind and envelope representation differ".into(),
            ));
        }
        if legacy {
            crate::migration::check_legacy_version(&key.schema_version)?;
        }
        match self.registrations.entry(key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(registration);
                Ok(())
            }
            Entry::Occupied(_) => Err(Error::Validation(
                "duplicate document codec registration".into(),
            )),
        }
    }

    pub fn get(&self, key: &CodecKey) -> Option<&CodecRegistration> {
        self.registrations.get(key)
    }

    /// Inspect the container and advertise owner capabilities. This does not run domain validation.
    pub fn inspect<'a>(
        &self,
        entry: &DocumentEntry,
        bytes: &'a [u8],
        limits: &Limits,
    ) -> Result<DocumentInspection<'a>> {
        let document = read_structure(entry, bytes, limits)?;
        let registration = self.registration_for(entry, &document);
        let document = if registration.is_none() && DocumentKind::built_in(&entry.kind).is_none() {
            DocumentRead::Opaque(bytes)
        } else {
            document
        };
        Ok(DocumentInspection {
            capabilities: DocumentCapabilities {
                structural_read: !matches!(document, DocumentRead::Opaque(_)),
                domain_support: registration.is_some(),
                byte_preservation: true,
                writable: registration.is_some_and(|codec| codec.writable),
                migratable: registration.is_some_and(|codec| codec.migratable),
            },
            document,
            original: bytes,
        })
    }

    /// Validate the original bytes with the owner codec for the exact representation.
    pub fn validate_domain(
        &self,
        entry: &DocumentEntry,
        bytes: &[u8],
        limits: &Limits,
    ) -> Result<()> {
        let document = read_structure(entry, bytes, limits)?;
        let codec = self
            .registration_for(entry, &document)
            .ok_or_else(|| unsupported("document domain", entry))?;
        (codec.validate)(entry, bytes)
    }

    /// Require opt-in for writes and validate the proposed bytes with the owner codec.
    pub fn validate_write(
        &self,
        entry: &DocumentEntry,
        bytes: &[u8],
        limits: &Limits,
    ) -> Result<()> {
        let document = read_structure(entry, bytes, limits)?;
        self.validate_decoded_write(entry, bytes, &document, false)
    }

    /// Require opt-in for migration and writes, then validate the migrated bytes.
    pub fn validate_migration(
        &self,
        entry: &DocumentEntry,
        bytes: &[u8],
        limits: &Limits,
    ) -> Result<()> {
        let document = read_structure(entry, bytes, limits)?;
        self.validate_decoded_write(entry, bytes, &document, true)
    }

    /// The caller must supply the structural result for this entry and these exact bytes.
    pub(crate) fn validate_decoded_write(
        &self,
        entry: &DocumentEntry,
        bytes: &[u8],
        document: &DocumentRead<'_>,
        migration: bool,
    ) -> Result<()> {
        let domain = if migration {
            "document migration"
        } else {
            "document write"
        };
        let codec = self
            .registration_for(entry, document)
            .filter(|codec| codec.writable && (!migration || codec.migratable))
            .ok_or_else(|| unsupported(domain, entry))?;
        (codec.validate)(entry, bytes)
    }

    fn registration_for(
        &self,
        entry: &DocumentEntry,
        document: &DocumentRead<'_>,
    ) -> Option<&CodecRegistration> {
        let envelope_version = match document {
            DocumentRead::Envelope(_) => EnvelopeVersion::V1,
            DocumentRead::LegacyCircuit(_) => EnvelopeVersion::LegacyCircuitGraph,
            DocumentRead::Opaque(_) => return None,
        };
        self.get(&CodecKey {
            kind: DocumentKind::from(entry.kind.as_str()),
            envelope_version,
            schema_version: entry.schema_version.clone(),
        })
    }
}

fn unsupported(domain: &'static str, entry: &DocumentEntry) -> Error {
    Error::Unsupported {
        domain,
        version: entry.schema_version.clone(),
    }
}
