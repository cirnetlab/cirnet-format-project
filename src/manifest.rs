use crate::{Error, Limits, ProjectPath, Result, error::check_limit, json, path::unique_paths};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub const MANIFEST_FORMAT: &str = "cirnet.project";
pub const FORMAT_VERSION: &str = "1.0";

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    Reference,
    Dependency,
    Library,
    Configuration,
    Result,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub relationship: Relationship,
    pub target: String,
}

#[derive(Clone, Copy, Debug)]
pub enum Resource<'a> {
    Document(&'a DocumentEntry),
    Asset(&'a AssetEntry),
    ExternalLibrary(&'a ExternalLibrary),
}

impl Resource<'_> {
    pub fn path(&self) -> Option<&ProjectPath> {
        match self {
            Self::Document(entry) => Some(&entry.path),
            Self::Asset(entry) => Some(&entry.path),
            Self::ExternalLibrary(_) => None,
        }
    }
}

/// Index once per manifest snapshot to resolve links without scanning the registry per lookup.
pub struct ReferenceIndex<'a> {
    by_id: HashMap<&'a str, Resource<'a>>,
}

impl<'a> ReferenceIndex<'a> {
    pub fn get(&self, id: &str) -> Option<Resource<'a>> {
        self.by_id.get(id).copied()
    }
    pub fn resolve(&self, link: &Link) -> Result<Resource<'a>> {
        let resource = self
            .get(&link.target)
            .ok_or_else(|| Error::Missing(link.target.clone()))?;
        if link.relationship == Relationship::Library && !kind_is_library(resource) {
            return Err(Error::Validation(format!(
                "library link targets {}",
                link.target
            )));
        }
        if matches!(
            link.relationship,
            Relationship::Configuration | Relationship::Result
        ) && matches!(resource, Resource::ExternalLibrary(_))
        {
            return Err(Error::Validation(format!(
                "configuration or result link targets external library {}",
                link.target
            )));
        }
        Ok(resource)
    }
}

fn kind_is_library(resource: Resource<'_>) -> bool {
    match resource {
        Resource::ExternalLibrary(_) => true,
        Resource::Document(entry) => entry.kind == "library",
        Resource::Asset(_) => false,
    }
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
            format_version: FORMAT_VERSION.into(),
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
        let mut index = ReferenceIndex {
            by_id: HashMap::with_capacity(count),
        };
        let mut paths = Vec::new();
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
            insert(&mut index, &doc.id, Resource::Document(doc))?;
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
            insert(&mut index, &asset.id, Resource::Asset(asset))?;
            paths.push(&asset.path);
        }
        for lib in &self.libraries {
            check_token(&lib.coordinate, "external library coordinate")?;
            check_token(&lib.version, "external library version")?;
            insert(&mut index, &lib.id, Resource::ExternalLibrary(lib))?;
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
        check_links(&index, "project", &self.links)?;
        for doc in &self.documents {
            check_links(&index, &doc.id, &doc.links)?;
        }
        check_dependencies(self, &index)?;
        Ok(index)
    }

    fn validate_header(&self) -> Result<()> {
        check_version(
            "project",
            &self.format,
            MANIFEST_FORMAT,
            &self.format_version,
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

fn is_media_part(value: &str) -> bool {
    (1..=127).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&byte))
}

fn insert<'a>(index: &mut ReferenceIndex<'a>, id: &'a str, value: Resource<'a>) -> Result<()> {
    check_token(id, "resource ID")?;
    if index.by_id.insert(id, value).is_some() {
        return Err(Error::Validation(format!("duplicate resource ID {id}")));
    }
    Ok(())
}

fn check_links(index: &ReferenceIndex<'_>, owner: &str, links: &[Link]) -> Result<()> {
    let mut unique = HashSet::new();
    for link in links {
        if !unique.insert(link) {
            return Err(Error::Validation(format!("duplicate link on {owner}")));
        }
        index.resolve(link)?;
    }
    Ok(())
}

// Kahn's algorithm scales with the number of documents and hard relationships.
fn check_dependencies(manifest: &Manifest, index: &ReferenceIndex<'_>) -> Result<()> {
    let mut inbound: HashMap<&str, usize> = manifest
        .documents
        .iter()
        .map(|doc| (doc.id.as_str(), 0))
        .collect();
    let dependencies = |doc: &DocumentEntry| {
        doc.links
            .iter()
            .filter(|link| {
                matches!(
                    link.relationship,
                    Relationship::Dependency | Relationship::Library
                ) && matches!(index.get(&link.target), Some(Resource::Document(_)))
            })
            .filter_map(|link| match index.get(&link.target) {
                Some(Resource::Document(target)) => Some(target.id.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for doc in &manifest.documents {
        for target in dependencies(doc) {
            *inbound.get_mut(target).expect("document indexed") += 1;
        }
    }
    let mut ready: VecDeque<_> = manifest
        .documents
        .iter()
        .filter(|doc| inbound[doc.id.as_str()] == 0)
        .collect();
    let mut visited = 0;
    while let Some(doc) = ready.pop_front() {
        visited += 1;
        for target in dependencies(doc) {
            let pending = inbound.get_mut(target).expect("document indexed");
            *pending -= 1;
            if *pending == 0
                && let Some(Resource::Document(next)) = index.get(target)
            {
                ready.push_back(next);
            }
        }
    }
    if visited != manifest.documents.len() {
        let blocked = manifest
            .documents
            .iter()
            .find(|doc| inbound[doc.id.as_str()] != 0)
            .expect("unvisited document");
        return Err(Error::Validation(format!(
            "dependency cycle blocks {}",
            blocked.id
        )));
    }
    Ok(())
}

pub(crate) fn is_token(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-'))
}

pub(crate) fn check_token(value: &str, label: &str) -> Result<()> {
    if !is_token(value) {
        return Err(Error::Validation(format!("invalid {label}")));
    }
    Ok(())
}

pub(crate) fn check_text(value: &str, label: &str, length: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > length
        || value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        return Err(Error::Validation(format!("invalid {label}")));
    }
    Ok(())
}

pub(crate) fn check_extensions(value: &Extensions) -> Result<()> {
    for key in value.keys() {
        if !is_token(key) || !key.split('.').all(is_token) || !key.contains('.') {
            return Err(Error::Validation(
                "expected dotted namespaces for extension data".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_version(
    domain: &'static str,
    format: &str,
    expected: &str,
    version: &str,
) -> Result<()> {
    if format != expected || version != FORMAT_VERSION {
        let version = if format == expected {
            version.to_owned()
        } else {
            format!("{format}/{version}")
        };
        return Err(Error::Unsupported { domain, version });
    }
    Ok(())
}

/// Returns the reserved extension of a built-in kind. Unknown kinds are portable as opaque documents.
pub fn document_extension(kind: &str) -> Option<&'static str> {
    match kind {
        "schematic" => Some("cirsch"),
        "pcb" => Some("cirpcb"),
        "assembly" => Some("cir3d"),
        "library" => Some("cirlib"),
        "circuit-graph" => Some("json"),
        _ => None,
    }
}

/// Read a manifest, reject unsupported versions, and check its complete reference structure.
pub fn decode_manifest(bytes: &[u8], limits: &Limits) -> Result<Manifest> {
    let value: Value = json::read(bytes, limits)?;
    let (format, version) = crate::migration::header(&value)?;
    check_version("project", format, MANIFEST_FORMAT, version)?;
    let manifest: Manifest = serde_json::from_value(value)?;
    manifest.index(limits)?;
    Ok(manifest)
}
