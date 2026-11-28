#![cfg(feature = "native")]

use cirnet_format_project::{
    AssetEntry, CodecRegistry, Digest, Document, DocumentEntry, Error, Limits, Link, Manifest,
    ProjectPath, Relationship, storage::Project, transaction::ChangeSet,
};
use std::{fs, io::Write, ops::ControlFlow};

fn document(id: &str) -> DocumentEntry {
    DocumentEntry {
        id: id.into(),
        kind: "schematic".into(),
        schema_version: "test.1".into(),
        path: ProjectPath::new(format!("{id}.cirsch")).unwrap(),
        title: None,
        links: Vec::new(),
        extensions: Default::default(),
    }
}

fn add_document(folder: &std::path::Path, manifest: &mut Manifest, entry: DocumentEntry) {
    let bytes = Document::new(&entry, serde_json::json!({"fixture": true}))
        .to_bytes(&Limits::default())
        .unwrap();
    fs::write(folder.join(entry.path.as_path()), bytes).unwrap();
    manifest.documents.push(entry);
}

fn configured(limits: Limits) -> (tempfile::TempDir, Project) {
    let area = tempfile::tempdir().unwrap();
    let mut manifest = Manifest::new("precad", "Pre-CAD fixture", vec!["electronics".into()]);
    let mut drawing = document("drawing");
    drawing.links.push(Link {
        relationship: Relationship::Dependency,
        target: "detail".into(),
    });
    add_document(area.path(), &mut manifest, drawing);
    add_document(area.path(), &mut manifest, document("detail"));
    let project = Project::create(area.path().join("lab.cirproj"), manifest, limits).unwrap();
    (area, project)
}

fn root_edit(project: &Project, links: Vec<Link>) -> ChangeSet<&'static [u8]> {
    let mut next = project.manifest().clone();
    next.links = links;
    let mut change = ChangeSet::new(project.revision().clone());
    change.set_manifest(next);
    change
}

#[test]
fn root_dependency_changes_require_the_affected_closure_only() {
    let (_area, mut project) = configured(Limits::default());
    let root_dependency = Link {
        relationship: Relationship::Dependency,
        target: "drawing".into(),
    };
    let add = root_edit(&project, vec![root_dependency.clone()]);
    assert!(matches!(
        project.apply_with_outcome(add, &CodecRegistry::new()),
        Err(Error::Validation(_))
    ));

    let mut partial = root_edit(&project, vec![root_dependency.clone()]);
    partial
        .include_read_set(&project.read_revisions(&["drawing"]).unwrap())
        .unwrap();
    assert!(matches!(
        project.apply_with_outcome(partial, &CodecRegistry::new()),
        Err(Error::Validation(message)) if message.contains("detail")
    ));

    let stale = project.read_revisions(&["drawing", "detail"]).unwrap();
    let detail = &project.manifest().documents[1];
    let bytes = Document::new(detail, serde_json::json!({"fixture": false}))
        .to_bytes(project.limits())
        .unwrap();
    project
        .save_document(
            "detail",
            stale.resource_revision("detail").unwrap(),
            &bytes,
            |_, _| Ok(()),
        )
        .unwrap();

    let mut stale_change = root_edit(&project, vec![root_dependency.clone()]);
    stale_change.include_read_set(&stale).unwrap();
    assert!(matches!(
        project.apply_with_outcome(stale_change, &CodecRegistry::new()),
        Err(Error::Conflict(id)) if id == "detail"
    ));
    assert!(project.manifest().links.is_empty());

    let mut coherent = root_edit(&project, vec![root_dependency.clone()]);
    coherent
        .include_read_set(&project.read_revisions(&["drawing", "detail"]).unwrap())
        .unwrap();
    project
        .apply_with_outcome(coherent, &CodecRegistry::new())
        .unwrap();

    let mut detach = root_edit(&project, vec![]);
    let no_tokens = root_edit(&project, vec![]);
    assert!(matches!(
        project.apply_with_outcome(no_tokens, &CodecRegistry::new()),
        Err(Error::Validation(_))
    ));
    detach
        .include_read_set(&project.read_revisions(&["drawing", "detail"]).unwrap())
        .unwrap();
    project
        .apply_with_outcome(detach, &CodecRegistry::new())
        .unwrap();

    // A soft project relationship declares metadata and never reads resource bytes.
    fs::remove_file(project.root().join("drawing.cirsch")).unwrap();
    let soft = root_edit(
        &project,
        vec![Link {
            relationship: Relationship::Result,
            target: "drawing".into(),
        }],
    );
    project
        .apply_with_outcome(soft, &CodecRegistry::new())
        .unwrap();
    assert_eq!(
        project.manifest().links[0].relationship,
        Relationship::Result
    );
}

#[test]
fn revision_only_capture_streams_assets_and_uses_a_separate_document_buffer_cap() {
    const ASSET_BYTES: u64 = 4 * 1024 * 1024;
    let (area, initial) = configured(Limits::default());
    let document_bytes = initial.read_document("detail").unwrap().bytes;
    let mut manifest = initial.manifest().clone();
    let asset = AssetEntry {
        id: "model".into(),
        path: ProjectPath::new("model.bin").unwrap(),
        media_type: "application/octet-stream".into(),
        extensions: Default::default(),
    };
    let path = area.path().join(asset.path.as_path());
    let mut asset_file = fs::File::create(&path).unwrap();
    for _ in 0..(ASSET_BYTES / 65536) {
        asset_file.write_all(&[0x72; 65536]).unwrap();
    }
    drop(asset_file);
    let asset_digest =
        cirnet_format_project::hash_reader(fs::File::open(&path).unwrap(), ASSET_BYTES)
            .unwrap()
            .1;
    manifest.assets.push(asset);
    let limits = Limits {
        max_total_bytes: ASSET_BYTES + document_bytes.len() as u64,
        ..Limits::default()
    };
    let project = Project::create(area.path().join("stream.cirproj"), manifest, limits).unwrap();
    let revisions = project.read_revisions(&["model", "detail"]).unwrap();
    assert_eq!(revisions.document("detail").map(|_| ()), None);
    assert_eq!(
        revisions.resource_revision("detail"),
        Some(&Digest::of(&document_bytes))
    );
    assert_eq!(revisions.resource_revision("model"), Some(&asset_digest));
    assert_eq!(
        revisions
            .resource_revisions()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec!["detail", "model"]
    );
    project.validate_read_set(&revisions).unwrap();

    let buffered = project
        .read_set_with_buffer_limit(&["model", "detail"], document_bytes.len() as u64)
        .unwrap();
    assert_eq!(buffered.document("detail").unwrap().bytes, document_bytes);
    assert_eq!(buffered.resource_revision("model"), Some(&asset_digest));
    assert!(matches!(
        project.read_set_with_buffer_limit(&["model", "detail"], document_bytes.len() as u64 - 1),
        Err(Error::Limit { .. })
    ));
    assert!(project.read_set_with_buffer_limit(&["model"], 0).is_ok());
    assert!(matches!(
        project.read_revisions(&["model", "detail", "drawing"]),
        Err(Error::Limit { .. })
    ));

    fs::write(area.path().join("detail.cirsch"), b"not a document").unwrap();
    assert!(
        matches!(project.validate_read_set(&revisions), Err(Error::Conflict(id)) if id=="detail")
    );
    assert!(project.read_set(&["detail"]).is_err());
    assert_eq!(
        project
            .read_revisions(&["detail"])
            .unwrap()
            .resource_revision("detail"),
        Some(&Digest::of(b"not a document"))
    );
}

#[test]
fn create_and_export_distinguish_publication_cancellation_and_conflict() {
    use cirnet_format_project::{package, transaction::DurabilityStatus};
    let area = tempfile::tempdir().unwrap();
    let target = area.path().join("lab.cirproj");
    let manifest = Manifest::new(
        "publication",
        "Publication fixture",
        vec!["electronics".into()],
    );
    let result =
        Project::create_with_outcome(&target, manifest.clone(), Limits::default()).unwrap();
    assert_eq!(result.project.manifest(), &manifest);
    assert!(target.is_file());
    assert!(result.publication.warning.is_none());
    let expected = if cfg!(unix) {
        DurabilityStatus::Confirmed
    } else {
        DurabilityStatus::PlatformLimited
    };
    assert_eq!(result.publication.durability, expected);

    let published_bytes = fs::read(&target).unwrap();
    let mut conflicting = manifest.clone();
    conflicting.name = "different".into();
    assert!(matches!(
        Project::create_with_outcome(&target, conflicting, Limits::default()),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(&target).unwrap(), published_bytes);

    let bundle = area.path().join("lab.cirpkg");
    assert!(matches!(
        package::export_file_with_progress_and_outcome(
            &bundle,
            &result.project,
            package::Compression::Stored,
            |_| ControlFlow::Break(())
        ),
        Err(Error::Cancelled)
    ));
    assert!(!bundle.exists());
    let outcome =
        package::export_file_with_outcome(&bundle, &result.project, package::Compression::Stored)
            .unwrap();
    assert_eq!(outcome.durability, expected);
    assert!(outcome.warning.is_none());
    assert_eq!(
        package::verify_only(fs::File::open(&bundle).unwrap(), Limits::default()).unwrap(),
        manifest
    );

    let archive_bytes = fs::read(&bundle).unwrap();
    assert!(matches!(
        package::export_file_with_outcome(&bundle, &result.project, package::Compression::Stored),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(bundle).unwrap(), archive_bytes);
}

#[test]
fn structured_rejection_reports_inspection_before_retry() {
    use cirnet_format_project::transaction::RecoveryRequirement;
    let (area, mut project) = configured(Limits::default());
    let registry = CodecRegistry::new();
    let invalid = root_edit(
        &project,
        vec![Link {
            relationship: Relationship::Reference,
            target: "unknown".into(),
        }],
    );
    let result = project.apply_report(invalid, &registry).unwrap_err();
    assert!(matches!(result.cause, Error::Missing(_)));
    assert_eq!(result.recovery, RecoveryRequirement::Clean);

    let staging = area.path().join(".cirnet-transaction");
    fs::create_dir(&staging).unwrap();
    let pending = root_edit(&project, vec![]);
    let result = project.apply_report(pending, &registry).unwrap_err();
    assert!(matches!(result.cause, Error::RecoveryRequired));
    assert_eq!(result.recovery, RecoveryRequirement::Inspect);
    assert!(staging.is_dir());
}
