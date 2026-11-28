use super::{Link, ProjectPath, Relationship, Resource};
use crate::{DocumentEntry, Error, Result, manifest::check_token};
use std::collections::{HashMap, HashSet, VecDeque};

/// The project itself is distinct from a document with the same ID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceOwner<'a> {
    Project,
    Document(&'a str),
}

/// An authored relationship attached to its owner and resolved target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceEdge<'a> {
    pub owner: ResourceOwner<'a>,
    pub link: &'a Link,
    pub target: Resource<'a>,
}

/// Owners to consider when relocating or removing a registered resource.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceImpact<'a> {
    pub resource: Resource<'a>,
    pub incoming: Vec<ResourceEdge<'a>>,
    pub dependent_owners: Vec<ResourceOwner<'a>>,
}

/// Validated ID, path and relationship indexes borrowed from a manifest snapshot.
#[derive(Debug)]
pub struct ResourceGraph<'a> {
    by_id: HashMap<&'a str, Resource<'a>>,
    by_path: HashMap<&'a ProjectPath, Resource<'a>>,
    project_outgoing: Vec<ResourceEdge<'a>>,
    document_outgoing: HashMap<&'a str, Vec<ResourceEdge<'a>>>,
    incoming: HashMap<&'a str, Vec<ResourceEdge<'a>>>,
}

/// The original reference lookup name remains available for existing callers.
pub type ReferenceIndex<'a> = ResourceGraph<'a>;

impl<'a> ResourceGraph<'a> {
    pub(crate) fn new(resources: usize, local_paths: usize) -> Self {
        Self {
            by_id: HashMap::with_capacity(resources),
            by_path: HashMap::with_capacity(local_paths),
            project_outgoing: Vec::new(),
            document_outgoing: HashMap::new(),
            incoming: HashMap::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<Resource<'a>> {
        self.by_id.get(id).copied()
    }

    /// Look up a local resource using the registered case-sensitive spelling.
    pub fn get_by_path(&self, path: &ProjectPath) -> Option<Resource<'a>> {
        self.by_path.get(path).copied()
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

    /// Authored edges in the owner's manifest order.
    pub fn outgoing(&self, owner: ResourceOwner<'_>) -> &[ResourceEdge<'a>] {
        match owner {
            ResourceOwner::Project => &self.project_outgoing,
            ResourceOwner::Document(id) => {
                self.document_outgoing.get(id).map_or(&[], Vec::as_slice)
            }
        }
    }

    /// Edges targeting this ID, with project links first and then document links in manifest order.
    pub fn incoming(&self, id: &str) -> &[ResourceEdge<'a>] {
        self.incoming.get(id).map_or(&[], Vec::as_slice)
    }

    /// Hard dependencies in breadth-first discovery order, excluding the starting resource.
    pub fn dependency_closure(&self, id: &str) -> Result<Vec<Resource<'a>>> {
        let root = self.get(id).ok_or_else(|| Error::Missing(id.into()))?;
        let mut dependencies = Vec::new();
        walk_hard_dependencies(
            [root],
            Resource::id,
            |owner| {
                self.outgoing(ResourceOwner::Document(owner.id()))
                    .iter()
                    .map(|edge| (edge.link.relationship, edge.target))
            },
            |resource| {
                if resource.id() != root.id() {
                    dependencies.push(*resource);
                }
                Ok(())
            },
        )?;
        Ok(dependencies)
    }

    /// Direct incoming edges and transitive owners across all relationships, including soft
    /// references. IDs make those links independent of a relocation's physical path.
    pub fn impact(&self, id: &str) -> Result<ResourceImpact<'a>> {
        let resource = self.get(id).ok_or_else(|| Error::Missing(id.into()))?;
        let mut seen = HashSet::from([ResourceOwner::Document(resource.id())]);
        let mut pending = VecDeque::from([resource.id()]);
        let mut dependent_owners = Vec::new();
        while let Some(target) = pending.pop_front() {
            for edge in self.incoming(target) {
                if seen.insert(edge.owner) {
                    if let ResourceOwner::Document(owner) = edge.owner {
                        pending.push_back(owner);
                    }
                    dependent_owners.push(edge.owner);
                }
            }
        }
        Ok(ResourceImpact {
            resource,
            incoming: self.incoming(id).to_vec(),
            dependent_owners,
        })
    }

    pub(crate) fn insert(&mut self, resource: Resource<'a>) -> Result<()> {
        let id = resource.id();
        check_token(id, "resource ID")?;
        if self.by_id.insert(id, resource).is_some() {
            return Err(Error::Validation(format!("duplicate resource ID {id}")));
        }
        if let Some(path) = resource.path() {
            self.by_path.insert(path, resource);
        }
        Ok(())
    }

    pub(crate) fn add_links(&mut self, owner: ResourceOwner<'a>, links: &'a [Link]) -> Result<()> {
        if links.is_empty() {
            return Ok(());
        }
        let mut unique = HashSet::with_capacity(links.len());
        let mut edges = Vec::with_capacity(links.len());
        for link in links {
            if !unique.insert(link) {
                let label = match owner {
                    ResourceOwner::Project => "project",
                    ResourceOwner::Document(id) => id,
                };
                return Err(Error::Validation(format!("duplicate link on {label}")));
            }
            let target = self.resolve(link)?;
            let edge = ResourceEdge {
                owner,
                link,
                target,
            };
            edges.push(edge);
            self.incoming.entry(target.id()).or_default().push(edge);
        }
        match owner {
            ResourceOwner::Project => self.project_outgoing = edges,
            ResourceOwner::Document(id) => {
                self.document_outgoing.insert(id, edges);
            }
        }
        Ok(())
    }

    // Kahn's algorithm uses the same resolved edges as queries and ignores soft relationships.
    pub(crate) fn check_dependencies(&self, documents: &[DocumentEntry]) -> Result<()> {
        let mut inbound = HashMap::new();
        for edges in self.document_outgoing.values() {
            for id in edges.iter().filter_map(hard_document_target) {
                *inbound.entry(id).or_insert(0usize) += 1;
            }
        }
        if inbound.is_empty() {
            return Ok(());
        }
        let mut ready: VecDeque<_> = documents
            .iter()
            .map(|doc| doc.id.as_str())
            .filter(|id| !inbound.contains_key(id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop_front() {
            visited += 1;
            for target in self
                .outgoing(ResourceOwner::Document(id))
                .iter()
                .filter_map(hard_document_target)
            {
                let pending = inbound.get_mut(target).expect("hard edge counted");
                *pending -= 1;
                if *pending == 0 {
                    ready.push_back(target);
                }
            }
        }
        if visited != documents.len() {
            let blocked = documents
                .iter()
                .find(|doc| {
                    inbound
                        .get(doc.id.as_str())
                        .is_some_and(|&count| count != 0)
                })
                .expect("unvisited document");
            return Err(Error::Validation(format!(
                "dependency cycle blocks {}",
                blocked.id
            )));
        }
        Ok(())
    }
}

/// Shared hard-dependency traversal over either validated graph edges or manifest rows.
pub(crate) fn walk_hard_dependencies<'a, N, I>(
    roots: impl IntoIterator<Item = N>,
    key: impl Fn(&N) -> &'a str,
    links: impl Fn(&N) -> I,
    mut visit: impl FnMut(&N) -> Result<()>,
) -> Result<()>
where
    I: IntoIterator<Item = (Relationship, N)>,
{
    let mut seen = HashSet::new();
    let mut pending = VecDeque::new();
    for root in roots {
        if seen.insert(key(&root)) {
            pending.push_back(root);
        }
    }
    while let Some(owner) = pending.pop_front() {
        visit(&owner)?;
        for (relationship, target) in links(&owner) {
            if relationship.is_hard() && seen.insert(key(&target)) {
                pending.push_back(target);
            }
        }
    }
    Ok(())
}

fn hard_document_target<'a>(edge: &ResourceEdge<'a>) -> Option<&'a str> {
    match edge.target {
        Resource::Document(entry) if edge.link.relationship.is_hard() => Some(&entry.id),
        _ => None,
    }
}

fn kind_is_library(resource: Resource<'_>) -> bool {
    match resource {
        Resource::ExternalLibrary(_) => true,
        Resource::Document(entry) => entry.kind == "library",
        Resource::Asset(_) => false,
    }
}
