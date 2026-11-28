use crate::{Digest, Error};

/// Whether the transaction advanced the visible project state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationStatus {
    Unchanged,
    Published,
}

/// Confirmation of the filesystem synchronization steps available on the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityStatus {
    NotApplicable,
    Confirmed,
    /// File synchronization completed but this adapter cannot confirm directory durability.
    PlatformLimited,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceChangeKind {
    Created,
    Replaced,
    Relocated,
    Migrated,
    Removed,
    Metadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceChange {
    pub id: String,
    pub kind: ResourceChangeKind,
    /// The resulting byte revision, when the resource still has a local file.
    pub revision: Option<Digest>,
}

/// Result of a completed write attempt. An outer error means the commit point
/// was not published. An interrupted stage may still require inspection.
#[derive(Debug)]
pub struct TransactionOutcome {
    pub publication: PublicationStatus,
    pub manifest_revision: Digest,
    /// Affected resources ordered by stable ID.
    pub resources: Vec<ResourceChange>,
    pub durability: DurabilityStatus,
    /// Inspect recovery before attempting another write when this is true.
    pub recovery_required: bool,
    /// Any diagnostic returned after publication. The new revision is still effective.
    pub completion_error: Option<Error>,
}

impl TransactionOutcome {
    pub(crate) fn new(
        manifest_revision: Digest,
        published: bool,
        mut resources: Vec<ResourceChange>,
        completion_error: Option<Error>,
        recovery_required: bool,
    ) -> Self {
        resources.sort_unstable_by(|left, right| left.id.cmp(&right.id));
        Self {
            publication: if published {
                PublicationStatus::Published
            } else {
                PublicationStatus::Unchanged
            },
            manifest_revision,
            resources,
            durability: if !published {
                DurabilityStatus::NotApplicable
            } else if matches!(completion_error, Some(Error::Durability { .. })) {
                DurabilityStatus::Uncertain
            } else if cfg!(unix) {
                DurabilityStatus::Confirmed
            } else {
                DurabilityStatus::PlatformLimited
            },
            recovery_required,
            completion_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_reports_manifest_publication_despite_sync_uncertainty() {
        let revision = Digest::of(b"published revision");
        let outcome = TransactionOutcome::new(
            revision.clone(),
            true,
            vec![],
            Some(Error::Durability {
                context: "manifest".into(),
                source: std::io::Error::other("injected directory sync failure"),
            }),
            true,
        );
        assert_eq!(outcome.publication, PublicationStatus::Published);
        assert_eq!(outcome.manifest_revision, revision);
        assert_eq!(outcome.durability, DurabilityStatus::Uncertain);
        assert!(outcome.recovery_required);
        assert!(matches!(
            outcome.completion_error,
            Some(Error::Durability { .. })
        ));
    }
}
