//! Portable CIRNET LAB manifests, document envelopes, resource graphs and packages.
#![forbid(unsafe_code)]

pub mod document;
mod error;
mod integrity;
mod limits;
pub mod manifest;
mod migration;
pub mod resource;
mod serialization;

pub(crate) use resource::path;
pub(crate) use serialization::json;

pub use document::{
    CodecKey, CodecRegistration, CodecRegistry, DOCUMENT_ENVELOPE_FORMAT_VERSION,
    DOCUMENT_ENVELOPE_VERSION, DOCUMENT_FORMAT, Document, DocumentCapabilities, DocumentInspection,
    DocumentKind, DocumentRead, EnvelopeVersion, decode_document, document_extension,
    validate_document_transition,
};
pub use error::{Error, Result};
pub use integrity::{Digest, hash_reader};
pub use limits::Limits;
pub use manifest::{
    AssetEntry, DocumentEntry, Extensions, ExternalLibrary, FORMAT_VERSION, MANIFEST_FORMAT,
    MANIFEST_FORMAT_VERSION, Manifest, decode_manifest,
};
pub use migration::{Migration, detect_version, import_legacy_circuit, migrate_json};
pub use resource::{
    Link, ProjectPath, ReferenceIndex, Relationship, Resource, ResourceEdge, ResourceGraph,
    ResourceImpact, ResourceOwner,
};

#[cfg(feature = "package")]
pub mod package;
#[cfg(feature = "native")]
pub mod storage;
#[cfg(feature = "native")]
pub mod transaction;
