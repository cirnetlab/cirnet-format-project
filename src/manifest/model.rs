//! Project Format 1.0 manifest metadata and validation.

use super::validation::{check_extensions, check_text, check_token, check_version, is_media_part};
use crate::{
    Error, Limits, ProjectPath, Result,
    document::document_extension,
    error::check_limit,
    json,
    path::unique_paths,
    resource::{Link, ReferenceIndex, Resource, ResourceGraph, ResourceOwner},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub const MANIFEST_FORMAT: &str = "cirnet.project";
pub const MANIFEST_FORMAT_VERSION: &str = "1.0";
/// Compatibility alias for the Project Format 1.0 manifest version.
pub const FORMAT_VERSION: &str = MANIFEST_FORMAT_VERSION;

pub type Extensions = BTreeMap<String, Value>;

/// File and logical metadata used for inspection before any engineering document is loaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub format_version: String,
    pub project_id: String,
    pub name: String,
    pub domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub documents: Vec<DocumentEntry>,
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
    #[serde(default)]
    pub libraries: Vec<ExternalLibrary>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentEntry {
    pub id: String,
    pub kind: String,
    pub schema_version: String,
    pub path: ProjectPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetEntry {
    pub id: String,
    pub path: ProjectPath,
    pub media_type: String,
    #[serde(default)]
    pub extensions: Extensions,
}

/// Externally resolved library coordinate. Only optional entries can remain external in a package.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalLibrary {
    pub id: String,
    pub coordinate: String,
    pub version: String,
    pub required: bool,
}

impl Manifest {
    /// Create an empty project manifest with a caller-assigned stable ID.
    ///
    /// ```
    /// use cirnet_format_project::{Limits, Manifest, decode_manifest};
    /// let limits = Limits::default();
    /// let manifest = Manifest::new("lab-01", "Bench project", vec!["electronics".into()]);
    /// let encoded = manifest.to_bytes(&limits)?;
    /// assert_eq!(decode_manifest(&encoded, &limits)?, manifest);
    /// # Ok::<(), cirnet_format_project::Error>(())
    /// ```
    pub fn new(id: impl Into<String>, name: impl Into<String>, domains: Vec<String>) -> Self {
        Self {
            format: MANIFEST_FORMAT.into(),
            format_version: MANIFEST_FORMAT_VERSION.into(),
            project_id: id.into(),
            name: name.into(),
            domains,
            description: None,
            documents: vec![],
            assets: vec![],
            libraries: vec![],
            links: vec![],
            extensions: Extensions::new(),
        }
    }

    pub fn to_bytes(&self, limits: &Limits) -> Result<Vec<u8>> {
        self.index(limits)?;
        json::write(self, limits)
    }

    pub fn index<'a>(&'a self, limits: &Limits) -> Result<ReferenceIndex<'a>> {
        self.resource_graph(limits)
    }

    /// Validate the manifest and build reusable ID, path and relationship indexes.
    pub fn resource_graph(&self, limits: &Limits) -> Result<ResourceGraph<'_>> {
        self.validate_header()?;
        let count = self
            .documents
            .len()
            .saturating_add(self.assets.len())
            .saturating_add(self.libraries.len());
        check_limit(
            "registered resources",
            count as u64,
            limits.max_resources as u64,
        )?;
        let relationships = self
            .documents
            .iter()
            .fold(self.links.len(), |n, doc| n.saturating_add(doc.links.len()));
        check_limit(
            "relationships",
            relationships as u64,
            limits.max_relationships as u64,
        )?;
        let local_count = self.documents.len() + self.assets.len();
        let mut graph = ResourceGraph::new(count, local_count);
        let mut paths = Vec::with_capacity(local_count);
        for doc in &self.documents {
            check_token(&doc.kind, "document kind")?;
            check_token(&doc.schema_version, "document schema version")?;
            if let Some(title) = &doc.title {
                check_text(title, "document title", 256)?;
            }
            check_extensions(&doc.extensions)?;
            if let Some(ext) = document_extension(&doc.kind)
                && doc.path.extension() != Some(ext)
            {
                return Err(Error::Validation(format!(
                    "{} requires a .{ext} document",
                    doc.kind
                )));
            }
            graph.insert(Resource::Document(doc))?;
            paths.push(&doc.path);
        }
        for asset in &self.assets {
            if !asset
                .media_type
                .split_once('/')
                .is_some_and(|(a, b)| is_media_part(a) && is_media_part(b))
            {
                return Err(Error::Validation(format!(
                    "invalid media type for {}",
                    asset.id
                )));
            }
            check_extensions(&asset.extensions)?;
            graph.insert(Resource::Asset(asset))?;
            paths.push(&asset.path);
        }
        for lib in &self.libraries {
            check_token(&lib.coordinate, "external library coordinate")?;
            check_token(&lib.version, "external library version")?;
            graph.insert(Resource::ExternalLibrary(lib))?;
        }
        for path in &paths {
            let first = path
                .as_str()
                .split('/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(first.as_str(), "integrity.json" | "project.cirproj") {
                return Err(Error::Path(path.to_string()));
            }
        }
        unique_paths(paths)?;
        graph.add_links(ResourceOwner::Project, &self.links)?;
        for doc in &self.documents {
            graph.add_links(ResourceOwner::Document(&doc.id), &doc.links)?;
        }
        graph.check_dependencies(&self.documents)?;
        Ok(graph)
    }

    fn validate_header(&self) -> Result<()> {
        check_version(
            "project",
            &self.format,
            MANIFEST_FORMAT,
            &self.format_version,
            MANIFEST_FORMAT_VERSION,
        )?;
        check_token(&self.project_id, "project ID")?;
        check_text(&self.name, "project name", 256)?;
        if let Some(description) = &self.description {
            check_text(description, "project description", 4096)?;
        }
        if self.domains.is_empty() || self.domains.len() > 32 {
            return Err(Error::Validation("expected one to 32 domains".into()));
        }
        let mut domains = HashSet::new();
        for domain in &self.domains {
            check_token(domain, "engineering domain")?;
            if !domains.insert(domain) {
                return Err(Error::Validation("duplicate engineering domain".into()));
            }
        }
        check_extensions(&self.extensions)
    }
}
