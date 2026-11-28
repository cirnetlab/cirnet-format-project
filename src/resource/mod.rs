//! Stable resource identifiers, portable paths and validated relationship graphs.

mod graph;
pub(crate) mod path;

pub use graph::{ReferenceIndex, ResourceEdge, ResourceGraph, ResourceImpact, ResourceOwner};
pub use path::ProjectPath;

use crate::{AssetEntry, DocumentEntry, ExternalLibrary};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    Reference,
    Dependency,
    Library,
    Configuration,
    Result,
}

impl Relationship {
    /// Hard links express dependencies; other links may participate in cycles.
    pub fn is_hard(self) -> bool {
        matches!(self, Self::Dependency | Self::Library)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub relationship: Relationship,
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resource<'a> {
    Document(&'a DocumentEntry),
    Asset(&'a AssetEntry),
    ExternalLibrary(&'a ExternalLibrary),
}

impl<'a> Resource<'a> {
    pub fn id(&self) -> &'a str {
        match self {
            Self::Document(entry) => &entry.id,
            Self::Asset(entry) => &entry.id,
            Self::ExternalLibrary(entry) => &entry.id,
        }
    }

    pub fn path(&self) -> Option<&'a ProjectPath> {
        match self {
            Self::Document(entry) => Some(&entry.path),
            Self::Asset(entry) => Some(&entry.path),
            Self::ExternalLibrary(_) => None,
        }
    }
}
