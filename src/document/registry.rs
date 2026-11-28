use super::{DocumentKind, DocumentRead, EnvelopeVersion, read_structure};
use crate::{DocumentEntry, Error, Limits, Result, manifest::check_token};
use std::collections::{HashMap, HashSet, hash_map::Entry};

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

/// An owner-approved transition between two exact document codec keys.
#[derive(Clone, Debug)]
pub struct DocumentMigration {
    pub source: CodecKey,
    pub target: CodecKey,
    /// Validate the exact source and proposed destination, including domain transformation rules.
    pub validate_transition: MigrationValidator,
}

/// A domain-owned validator for one migration edge.
pub type MigrationValidator = fn(&DocumentEntry, &[u8], &DocumentEntry, &[u8]) -> Result<()>;

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
    migrations: HashMap<CodecKey, HashMap<CodecKey, MigrationValidator>>,
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

    /// Register a directed migration between already registered, compatible owner codecs.
    /// Multiple distinct destinations are allowed because each operation names its exact target.
    pub fn register_migration(&mut self, migration: DocumentMigration) -> Result<()> {
        let DocumentMigration {
            source,
            target,
            validate_transition,
        } = migration;
        if source.kind != target.kind || source == target {
            return Err(Error::Validation(
                "invalid document migration endpoints".into(),
            ));
        }
        if !self.get(&source).is_some_and(|codec| codec.migratable)
            || !self
                .get(&target)
                .is_some_and(|codec| codec.migratable && codec.writable)
        {
            return Err(Error::Validation(
                "migration endpoints require compatible owner codecs".into(),
            ));
        }
        let mut pending = vec![&target];
        let mut visited = HashSet::new();
        while let Some(endpoint) = pending.pop() {
            if endpoint == &source {
                return Err(Error::Validation("document migration cycle".into()));
            }
            if visited.insert(endpoint)
                && let Some(edges) = self.migrations.get(endpoint)
            {
                pending.extend(edges.keys());
            }
        }
        match self.migrations.entry(source).or_default().entry(target) {
            Entry::Vacant(entry) => {
                entry.insert(validate_transition);
                Ok(())
            }
            Entry::Occupied(_) => Err(Error::Validation(
                "duplicate document migration transition".into(),
            )),
        }
    }

    /// Check an exact owner migration edge, preservation, and both domain validators.
    pub fn validate_transition(
        &self,
        source: &DocumentEntry,
        original: &[u8],
        target: &DocumentEntry,
        proposed: &[u8],
        limits: &Limits,
    ) -> Result<()> {
        let before = read_structure(source, original, limits)?;
        let after = read_structure(target, proposed, limits)?;
        self.validate_decoded_transition((source, original, &before), (target, proposed, &after))?;
        self.validate_decoded_write(target, proposed, &after, true)
    }

    pub(crate) fn validate_decoded_transition(
        &self,
        before: (&DocumentEntry, &[u8], &DocumentRead<'_>),
        after: (&DocumentEntry, &[u8], &DocumentRead<'_>),
    ) -> Result<()> {
        let (source, original, previous) = before;
        let (target, proposed, next) = after;
        if source.id != target.id || source.kind != target.kind {
            return Err(Error::Validation(
                "document migration changed resource identity or kind".into(),
            ));
        }
        let source_key = key_for(source, previous)
            .ok_or_else(|| unsupported("document migration source", source))?;
        let target_key = key_for(target, next)
            .ok_or_else(|| unsupported("document migration target", target))?;
        let transition = self
            .migrations
            .get(&source_key)
            .and_then(|edges| edges.get(&target_key))
            .ok_or_else(|| Error::Unsupported {
                domain: "document migration transition",
                version: format!("{} -> {}", source.schema_version, target.schema_version),
            })?;
        super::validate_document_transition(target, previous, next)?;
        let codec = self
            .get(&source_key)
            .ok_or_else(|| unsupported("document migration source", source))?;
        (codec.validate)(source, original)?;
        transition(source, original, target, proposed)
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

    /// Check destination capabilities and domain bytes in isolation.
    /// Use validate_transition to authorize an actual source-to-target migration.
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
        self.get(&key_for(entry, document)?)
    }
}

fn key_for(entry: &DocumentEntry, document: &DocumentRead<'_>) -> Option<CodecKey> {
    let envelope_version = match document {
        DocumentRead::Envelope(_) => EnvelopeVersion::V1,
        DocumentRead::LegacyCircuit(_) => EnvelopeVersion::LegacyCircuitGraph,
        DocumentRead::Opaque(_) => return None,
    };
    Some(CodecKey {
        kind: DocumentKind::from(entry.kind.as_str()),
        envelope_version,
        schema_version: entry.schema_version.clone(),
    })
}

fn unsupported(domain: &'static str, entry: &DocumentEntry) -> Error {
    Error::Unsupported {
        domain,
        version: entry.schema_version.clone(),
    }
}
