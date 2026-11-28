#![cfg(feature = "native")]
use cirnet_format_project::storage::{Project, StagedFile};
use cirnet_format_project::{Document, DocumentEntry, Error, Limits, Manifest, ProjectPath};
use std::fs;

fn prepared(opaque: bool) -> (tempfile::TempDir, Project) {
    let root = tempfile::tempdir().unwrap();
    let limits = Limits::default();
    let mut manifest = Manifest::new("lab-01", "Project", vec!["electronics".into()]);
    let entry = DocumentEntry {
        id: "drawing".into(),
        path: ProjectPath::new("drawing.cirsch").unwrap(),
        kind: "schematic".into(),
        schema_version: "fixture.1".into(),
        title: None,
        links: vec![],
        extensions: Default::default(),
    };
    let bytes = if opaque {
        br#"{"format":"cirnet.document","format_version":"9.0","future":true}"#.to_vec()
    } else {
        Document::new(&entry, serde_json::json!({"value":0}))
            .to_bytes(&limits)
            .unwrap()
    };
    fs::write(root.path().join(entry.path.as_path()), bytes).unwrap();
    manifest.documents.push(entry);
    let project = Project::create(root.path().join("lab.cirproj"), manifest, limits).unwrap();
    (root, project)
}

#[test]
fn stale_legacy_commit_cannot_replace_a_newer_single_document_save() {
    let (root, project) = prepared(false);
    let mut old = Project::open(root.path().join("lab.cirproj"), Limits::default()).unwrap();
    let snapshot = project.read_document("drawing").unwrap();
    let entry = &project.manifest().documents[0];
    let latest = Document::new(entry, serde_json::json!({"value":1}))
        .to_bytes(project.limits())
        .unwrap();
    project
        .save_document("drawing", &snapshot.revision, &latest, |_, _| Ok(()))
        .unwrap();
    let mut next = old.manifest().clone();
    next.documents[0].path = ProjectPath::new("replacement.cirsch").unwrap();
    assert!(matches!(
        old.commit(
            next.clone(),
            vec![StagedFile::new(
                next.documents[0].path.clone(),
                snapshot.bytes.as_slice()
            )]
        ),
        Err(Error::Conflict(_) | Error::Validation(_))
    ));
    assert_eq!(project.read_document("drawing").unwrap().bytes, latest);
}

#[test]
fn legacy_commit_cannot_overwrite_an_opaque_document_at_a_new_path() {
    let (_root, mut project) = prepared(true);
    let mut next = project.manifest().clone();
    next.documents[0].path = ProjectPath::new("rewritten.cirsch").unwrap();
    let bytes = Document::new(&next.documents[0], serde_json::Value::Null)
        .to_bytes(project.limits())
        .unwrap();
    assert!(
        project
            .commit(
                next.clone(),
                vec![StagedFile::new(
                    next.documents[0].path.clone(),
                    bytes.as_slice()
                )]
            )
            .is_err()
    );
    assert!(!project.root().join("rewritten.cirsch").exists());
}

use cirnet_format_project::transaction::{ChangeSet, RecoveryState, ResourceWrite, WriteMode};
use cirnet_format_project::{
    CodecKey, CodecRegistration, CodecRegistry, DocumentKind, EnvelopeVersion, Link, Relationship,
};
use std::io::{Cursor, Read};

fn registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    registry
        .register(CodecRegistration {
            key: CodecKey {
                kind: DocumentKind::Schematic,
                envelope_version: EnvelopeVersion::V1,
                schema_version: "fixture.1".into(),
            },
            validate: |_, bytes| {
                let value: serde_json::Value = serde_json::from_slice(bytes)?;
                if value["payload"]["value"].is_i64() {
                    Ok(())
                } else {
                    Err(Error::Validation("owner expects integer value".into()))
                }
            },
            writable: true,
            migratable: false,
        })
        .unwrap();
    registry
}

fn entry(id: &str, path: &str) -> DocumentEntry {
    DocumentEntry {
        id: id.into(),
        kind: "schematic".into(),
        schema_version: "fixture.1".into(),
        path: ProjectPath::new(path).unwrap(),
        title: None,
        links: vec![],
        extensions: Default::default(),
    }
}

#[test]
fn affected_revisions_and_hard_dependencies_prevent_stale_updates() {
    let (_root, mut project) = prepared(false);
    let codec = registry();
    let mut next = project.manifest().clone();
    next.documents.push(entry("consumer", "consumer.cirsch"));
    next.documents[1].links.push(Link {
        relationship: Relationship::Dependency,
        target: "drawing".into(),
    });
    let doc = &next.documents[1];
    let bytes = Document::new(doc, serde_json::json!({"value":12}))
        .to_bytes(project.limits())
        .unwrap();
    let mut create = ChangeSet::new(project.revision().clone());
    create.expect("drawing", project.resource_revision("drawing").unwrap());
    create.write(ResourceWrite::new(
        "consumer",
        doc.path.clone(),
        WriteMode::Create,
        bytes.as_slice(),
    ));
    create.set_manifest(next);
    project.apply(create, &codec).unwrap();

    let old = project.read_document("drawing").unwrap();
    let consumer = project.read_document("consumer").unwrap();
    let mut next = project.manifest().clone();
    next.documents[1].path = ProjectPath::new("consumer-next.cirsch").unwrap();
    let mut changes = ChangeSet::new(project.revision().clone());
    changes.expect("drawing", old.revision.clone());
    changes.expect("consumer", consumer.revision);
    changes.write(ResourceWrite::new(
        "consumer",
        next.documents[1].path.clone(),
        WriteMode::Relocate,
        consumer.bytes.as_slice(),
    ));
    changes.set_manifest(next);

    let updated = Document::new(
        &project.manifest().documents[0],
        serde_json::json!({"value":2}),
    )
    .to_bytes(project.limits())
    .unwrap();
    project
        .save_document("drawing", &old.revision, &updated, |_, _| Ok(()))
        .unwrap();
    assert!(matches!(project.apply(changes, &codec), Err(Error::Conflict(id)) if id == "drawing"));
    assert_eq!(project.read_document("drawing").unwrap().bytes, updated);
    assert!(!project.root().join("consumer-next.cirsch").exists());
}

#[test]
fn opaque_relocation_requires_exact_bytes_and_deletion_is_explicit() {
    let (_root, mut project) = prepared(true);
    let codec = CodecRegistry::new();
    let original = project.read_document("drawing").unwrap();
    let mut next = project.manifest().clone();
    next.documents[0].path = ProjectPath::new("relocated.cirsch").unwrap();
    let mut replace = ChangeSet::new(project.revision().clone());
    replace.expect("drawing", original.revision.clone());
    let rewrite = Document::new(&next.documents[0], serde_json::Value::Null)
        .to_bytes(project.limits())
        .unwrap();
    replace.write(ResourceWrite::new(
        "drawing",
        next.documents[0].path.clone(),
        WriteMode::Replace,
        rewrite.as_slice(),
    ));
    replace.set_manifest(next.clone());
    assert!(matches!(
        project.apply(replace, &codec),
        Err(Error::Unsupported { .. })
    ));

    let mut relocate = ChangeSet::new(project.revision().clone());
    relocate.expect("drawing", original.revision.clone());
    relocate.write(ResourceWrite::new(
        "drawing",
        next.documents[0].path.clone(),
        WriteMode::Relocate,
        original.bytes.as_slice(),
    ));
    relocate.set_manifest(next);
    project.apply(relocate, &codec).unwrap();
    assert_eq!(
        project.read_document("drawing").unwrap().bytes,
        original.bytes
    );
    let mut next = project.manifest().clone();
    next.documents.clear();
    let mut remove = ChangeSet::<&[u8]>::new(project.revision().clone());
    remove.set_manifest(next);
    remove.expect("drawing", original.revision);
    remove.remove("drawing");
    project.apply(remove, &codec).unwrap();
    assert!(project.root().join("relocated.cirsch").is_file());
    assert!(matches!(
        project.read_document("drawing"),
        Err(Error::Missing(_))
    ));
}

#[test]
fn interrupted_publication_retains_external_files_and_leaves_previous_manifest() {
    let (_root, mut project) = prepared(false);
    let codec = registry();
    let mut next = project.manifest().clone();
    next.documents.extend([
        entry("first", "first.cirsch"),
        entry("second", "second.cirsch"),
    ]);
    let mut changes: ChangeSet<Box<dyn Read>> = ChangeSet::new(project.revision().clone());
    for item in &next.documents[1..] {
        let bytes = Document::new(item, serde_json::json!({"value":42}))
            .to_bytes(project.limits())
            .unwrap();
        let input: Box<dyn Read> = if item.id == "second" {
            Box::new(Collision {
                path: project.root().join("second.cirsch"),
                bytes: Cursor::new(bytes),
            })
        } else {
            Box::new(Cursor::new(bytes))
        };
        changes.write(ResourceWrite::new(
            &item.id,
            item.path.clone(),
            WriteMode::Create,
            input,
        ));
    }
    changes.set_manifest(next);
    assert!(matches!(
        project.apply(changes, &codec),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        project.inspect_recovery().unwrap(),
        RecoveryState::BeforePublication
    );
    assert_eq!(project.manifest().documents.len(), 1);
    let report = project.recover().unwrap();
    assert_eq!(report.state, RecoveryState::BeforePublication);
    assert_eq!(report.retained.len(), 2);
    assert_eq!(
        fs::read(project.root().join("second.cirsch")).unwrap(),
        b"other writer"
    );
    assert_eq!(project.inspect_recovery().unwrap(), RecoveryState::Clean);
}

struct Collision {
    path: std::path::PathBuf,
    bytes: Cursor<Vec<u8>>,
}
impl Read for Collision {
    fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
        if !self.path.exists() {
            fs::write(&self.path, b"other writer")?;
        }
        self.bytes.read(target)
    }
}

#[test]
fn failed_staging_and_owner_rejection_leave_no_transaction() {
    let (_root, mut project) = prepared(false);
    let codec = registry();
    let mut next = project.manifest().clone();
    next.documents.push(entry("bad", "bad.cirsch"));
    let invalid = Document::new(&next.documents[1], serde_json::Value::Null)
        .to_bytes(project.limits())
        .unwrap();
    let mut change = ChangeSet::new(project.revision().clone());
    change.write(ResourceWrite::new(
        "bad",
        next.documents[1].path.clone(),
        WriteMode::Create,
        invalid.as_slice(),
    ));
    change.set_manifest(next.clone());
    assert!(
        matches!(project.apply(change, &codec), Err(Error::Validation(message)) if message.contains("integer"))
    );
    assert_eq!(project.inspect_recovery().unwrap(), RecoveryState::Clean);
    let mut change = ChangeSet::new(project.revision().clone());
    change.write(ResourceWrite::new(
        "bad",
        next.documents[1].path.clone(),
        WriteMode::Create,
        FailedRead,
    ));
    change.set_manifest(next);
    assert!(matches!(
        project.apply(change, &codec),
        Err(Error::Io { .. })
    ));
    assert_eq!(project.inspect_recovery().unwrap(), RecoveryState::Clean);
}

struct FailedRead;
impl Read for FailedRead {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("simulated read failure"))
    }
}

#[test]
fn external_manifest_edit_before_publication_keeps_the_interrupted_journal() {
    let (_root, mut project) = prepared(false);
    let codec = registry();
    let anchor = project.root().join("lab.cirproj");
    let mut external = project.manifest().clone();
    external.name = "External edit".into();
    let external_bytes = external.to_bytes(project.limits()).unwrap();
    let mut next = project.manifest().clone();
    next.documents.push(entry("new", "new.cirsch"));
    let bytes = Document::new(&next.documents[1], serde_json::json!({"value":12}))
        .to_bytes(project.limits())
        .unwrap();
    struct BeforePublish {
        change: Option<(std::path::PathBuf, Vec<u8>)>,
        bytes: Cursor<Vec<u8>>,
    }
    impl Read for BeforePublish {
        fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
            if let Some((path, bytes)) = self.change.take() {
                fs::write(path, bytes)?;
            }
            self.bytes.read(target)
        }
    }
    let mut edit = ChangeSet::new(project.revision().clone());
    edit.write(ResourceWrite::new(
        "new",
        next.documents[1].path.clone(),
        WriteMode::Create,
        BeforePublish {
            change: Some((anchor.clone(), external_bytes.clone())),
            bytes: Cursor::new(bytes),
        },
    ));
    edit.set_manifest(next);
    assert!(matches!(
        project.apply(edit, &codec),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(anchor).unwrap(), external_bytes);
    assert_eq!(project.inspect_recovery().unwrap(), RecoveryState::Diverged);
    assert_eq!(project.recover().unwrap().state, RecoveryState::Diverged);
    assert_eq!(project.manifest().documents.len(), 1);
}
