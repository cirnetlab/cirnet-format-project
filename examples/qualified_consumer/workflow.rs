use cirnet_format_project::{
    AssetEntry, CodecKey, CodecRegistration, CodecRegistry, Digest, Document, DocumentEntry,
    DocumentKind, DocumentMigration, EnvelopeVersion, Error, Limits, Link, Manifest, ProjectPath,
    Relationship, Resource,
    package::{
        Compression, MANIFEST_ENTRY, PackagePublishError, export_file, inspect, verify, verify_only,
    },
    storage::Project,
    transaction::{
        ChangeSet, PublicationStatus, RecoveryState, ResourceChangeKind, ResourceWrite, WriteMode,
    },
};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Cursor, Read, Write},
    ops::ControlFlow,
    path::{Path, PathBuf},
};

const ASSET_BYTES: &[u8] = b"standalone qualification asset";
type DemoResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run(directory: &Path) -> DemoResult<()> {
    let limits = Limits::default();
    fs::create_dir(directory)?;
    let working = directory.join("working");
    fs::create_dir(&working)?;
    let manifest_path = working.join("Lab.cirproj");
    let manifest = Manifest::new("qualification", "Qualification", vec!["electronics".into()]);
    let mut project = Project::create(&manifest_path, manifest, limits)?;
    let registry = test_owner()?;

    let entry = DocumentEntry {
        id: "drawing".into(),
        kind: "schematic".into(),
        schema_version: "qualification.1".into(),
        path: ProjectPath::new("documents/Main.cirsch")?,
        title: Some("Qualification schematic".into()),
        links: vec![Link {
            relationship: Relationship::Dependency,
            target: "evidence".into(),
        }],
        extensions: Default::default(),
    };
    let evidence = asset("evidence", "assets/evidence.bin")?;
    let mut original = Document::new(&entry, json!({"value": 7}));
    original
        .extensions
        .insert("qualification.owner".into(), json!({"keep": true}));
    let bytes = original.to_bytes(&limits)?;

    let mut next = project.manifest().clone();
    next.documents.push(entry.clone());
    next.assets.push(evidence.clone());
    let mut create = ChangeSet::new(project.revision().clone());
    create.write(ResourceWrite::new(
        "drawing",
        entry.path.clone(),
        WriteMode::Create,
        bytes.as_slice(),
    ));
    create.write(ResourceWrite::new(
        "evidence",
        evidence.path.clone(),
        WriteMode::Create,
        ASSET_BYTES,
    ));
    create.set_manifest(next);
    let outcome = project.apply_with_outcome(create, &registry)?;
    assert_eq!(outcome.publication, PublicationStatus::Published);
    assert_eq!(outcome.manifest_revision, *project.revision());
    assert_eq!(outcome.resources.len(), 2);
    assert!(outcome.resources.iter().all(
        |resource| resource.kind == ResourceChangeKind::Created && resource.revision.is_some()
    ));
    if let Some(warning) = outcome.completion_error {
        return Err(warning.into());
    }

    let mut project = Project::open(&manifest_path, limits)?;
    let mut stale_project = Project::open(&manifest_path, limits)?;
    let graph = project.manifest().index(&limits)?;
    assert!(matches!(graph.get("drawing"), Some(Resource::Document(_))));
    assert!(matches!(
        graph.get_by_path(&evidence.path),
        Some(Resource::Asset(_))
    ));
    assert_eq!(graph.dependency_closure("drawing")?[0].id(), "evidence");

    let read = project.read_set(&["drawing", "evidence"])?;
    let stale = stale_project.read_set(&["drawing", "evidence"])?;
    let snapshot = read
        .document("drawing")
        .ok_or("drawing absent from read set")?;
    let inspection = registry.inspect(&entry, &snapshot.bytes, &limits)?;
    assert!(inspection.capabilities.domain_support && inspection.capabilities.writable);
    original.payload["value"] = json!(8);
    let edited = original.to_bytes(&limits)?;
    let mut change = ChangeSet::new(read.manifest_revision().clone());
    change.include_read_set(&read)?;
    change.write(ResourceWrite::new(
        "drawing",
        entry.path.clone(),
        WriteMode::Replace,
        edited.as_slice(),
    ));
    let saved = project.apply_with_outcome(change, &registry)?;
    assert_eq!(saved.publication, PublicationStatus::Published);
    assert_eq!(saved.resources[0].kind, ResourceChangeKind::Replaced);
    assert_eq!(
        saved.resources[0].revision.as_ref(),
        Some(&Digest::of(&edited))
    );
    if let Some(warning) = saved.completion_error {
        return Err(warning.into());
    }

    let mut stale_write = ChangeSet::new(stale.manifest_revision().clone());
    stale_write.include_read_set(&stale)?;
    stale_write.write(ResourceWrite::new(
        "drawing",
        entry.path.clone(),
        WriteMode::Replace,
        bytes.as_slice(),
    ));
    assert!(matches!(
        stale_project.validate_read_set(&stale),
        Err(Error::Conflict(_))
    ));
    match stale_project.apply_with_outcome(stale_write, &registry) {
        Err(Error::Conflict(_)) => {
            assert_eq!(stale_project.read_document("drawing")?.bytes, edited);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => return Err("stale write was unexpectedly accepted".into()),
    }

    let read = project.read_set(&["drawing", "evidence"])?;
    let mut migrated_entry = entry.clone();
    migrated_entry.schema_version = "qualification.2".into();
    migrated_entry.path = ProjectPath::new("documents/Main-v2.cirsch")?;
    let mut migrated = Document::new(
        &migrated_entry,
        json!({"value_v2": original.payload["value"]}),
    );
    migrated.extensions = original.extensions;
    let migrated_bytes = migrated.to_bytes(&limits)?;
    let mut next = project.manifest().clone();
    next.documents[0] = migrated_entry.clone();
    let migration_change = || -> cirnet_format_project::Result<_> {
        let mut migration = ChangeSet::new(read.manifest_revision().clone());
        migration.include_read_set(&read)?;
        migration.write(ResourceWrite::new(
            "drawing",
            migrated_entry.path.clone(),
            WriteMode::Migrate,
            migrated_bytes.as_slice(),
        ));
        migration.set_manifest(next.clone());
        Ok(migration)
    };
    assert!(matches!(
        project.apply_with_outcome(migration_change()?, &test_codecs()?),
        Err(Error::Unsupported {
            domain: "document migration transition",
            ..
        })
    ));
    assert_eq!(project.read_document("drawing")?.bytes, edited);
    let outcome = project.apply_with_outcome(migration_change()?, &registry)?;
    assert_eq!(outcome.publication, PublicationStatus::Published);
    assert_eq!(outcome.resources[0].kind, ResourceChangeKind::Migrated);
    if let Some(warning) = outcome.completion_error {
        return Err(warning.into());
    }

    qualify_transaction_recovery(&project, &manifest_path, &registry)?;
    qualify_package(directory, &project, &migrated_bytes)?;
    Ok(())
}

fn asset(id: &str, path: &str) -> cirnet_format_project::Result<AssetEntry> {
    Ok(AssetEntry {
        id: id.into(),
        path: ProjectPath::new(path)?,
        media_type: "application/octet-stream".into(),
        extensions: Default::default(),
    })
}

fn test_owner() -> cirnet_format_project::Result<CodecRegistry> {
    let mut registry = test_codecs()?;
    registry.register_migration(DocumentMigration {
        source: test_key("qualification.1"),
        target: test_key("qualification.2"),
        validate_transition: |_, original, _, migrated| {
            let original: Value = serde_json::from_slice(original)?;
            let migrated: Value = serde_json::from_slice(migrated)?;
            if original["payload"]["value"] == migrated["payload"]["value_v2"] {
                Ok(())
            } else {
                Err(Error::Validation(
                    "qualification owner rejected the migration".into(),
                ))
            }
        },
    })?;
    Ok(registry)
}

fn test_key(version: &str) -> CodecKey {
    CodecKey {
        kind: DocumentKind::Schematic,
        envelope_version: EnvelopeVersion::V1,
        schema_version: version.into(),
    }
}

fn test_codecs() -> cirnet_format_project::Result<CodecRegistry> {
    let mut registry = CodecRegistry::new();
    for version in ["qualification.1", "qualification.2"] {
        registry.register(CodecRegistration {
            key: test_key(version),
            validate: if version == "qualification.1" {
                valid_v1
            } else {
                valid_v2
            },
            writable: true,
            migratable: true,
        })?;
    }
    Ok(registry)
}

fn valid_v1(_: &DocumentEntry, bytes: &[u8]) -> cirnet_format_project::Result<()> {
    validate_number(bytes, "value")
}

fn valid_v2(_: &DocumentEntry, bytes: &[u8]) -> cirnet_format_project::Result<()> {
    validate_number(bytes, "value_v2")
}

fn validate_number(bytes: &[u8], field: &str) -> cirnet_format_project::Result<()> {
    let decoded: Value = serde_json::from_slice(bytes)?;
    if decoded["payload"][field].is_i64() {
        Ok(())
    } else {
        Err(Error::Validation(
            "qualification owner expected an integer".into(),
        ))
    }
}

fn qualify_transaction_recovery(
    project: &Project,
    manifest_path: &Path,
    registry: &CodecRegistry,
) -> DemoResult<()> {
    let mut reopened = Project::open(manifest_path, *project.limits())?;
    let first = asset("first", "interruption/first.bin")?;
    let second = asset("second", "interruption/external.bin")?;
    fs::create_dir(project.root().join("interruption"))?;
    let conflict = project.root().join(second.path.as_path());
    let mut next = reopened.manifest().clone();
    next.assets.extend([first.clone(), second.clone()]);
    let mut transaction: ChangeSet<Box<dyn Read>> = ChangeSet::new(reopened.revision().clone());
    transaction.write(ResourceWrite::new(
        "first",
        first.path,
        WriteMode::Create,
        Box::new(Cursor::new(b"staged")),
    ));
    transaction.write(ResourceWrite::new(
        "second",
        second.path,
        WriteMode::Create,
        Box::new(CreateConflict {
            conflict: Some(conflict.clone()),
            bytes: Cursor::new(b"proposed"),
        }),
    ));
    transaction.set_manifest(next);
    assert!(matches!(
        reopened.apply_with_outcome(transaction, registry),
        Err(Error::Conflict(_))
    ));
    let reopened = Project::open(manifest_path, *project.limits())?;
    assert_eq!(
        reopened.inspect_recovery()?,
        RecoveryState::BeforePublication
    );
    let recovery = reopened.recover()?;
    assert!(
        recovery
            .retained
            .iter()
            .any(|item| item.path.as_str() == "interruption/first.bin" && item.matches_staged)
    );
    assert!(
        recovery
            .retained
            .iter()
            .any(|item| item.path.as_str() == "interruption/external.bin" && !item.matches_staged)
    );
    assert_eq!(fs::read(conflict)?, b"external writer");
    assert_eq!(reopened.inspect_recovery()?, RecoveryState::Clean);
    assert_eq!(reopened.manifest(), project.manifest());
    Ok(())
}

struct CreateConflict {
    conflict: Option<PathBuf>,
    bytes: Cursor<&'static [u8]>,
}

impl Read for CreateConflict {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(path) = self.conflict.take() {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .write_all(b"external writer")?;
        }
        self.bytes.read(buffer)
    }
}

fn qualify_package(directory: &Path, project: &Project, migrated_bytes: &[u8]) -> DemoResult<()> {
    let archive = directory.join("Lab.cirpkg");
    let limits = *project.limits();
    export_file(&archive, project, Compression::Deflate)?;
    assert_eq!(&inspect(File::open(&archive)?, limits)?, project.manifest());
    assert_eq!(
        &verify_only(File::open(&archive)?, limits)?,
        project.manifest()
    );
    let mut verified = verify(File::open(archive)?, directory, limits)?;
    let destination = directory.join("imported");
    let cancelled = verified.publish_retryable(&destination, |event| {
        if event.member == Some(MANIFEST_ENTRY) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(
        cancelled,
        Err(PackagePublishError::Rejected(Error::Cancelled))
    ));
    assert!(!destination.exists());
    let outcome = verified.publish_retryable(&destination, |_| ControlFlow::Continue(()))?;
    if let Some(warning) = outcome.warning {
        return Err(warning.into());
    }
    let imported = outcome.project;
    let graph = imported.manifest().index(&limits)?;
    assert_eq!(graph.dependency_closure("drawing")?[0].id(), "evidence");
    let drawing = imported.read_document("drawing")?;
    assert_eq!(drawing.bytes, migrated_bytes);
    assert_eq!(drawing.revision, Digest::of(migrated_bytes));
    assert_eq!(
        imported.resource_revision("evidence")?,
        Digest::of(ASSET_BYTES)
    );
    assert_eq!(imported.manifest(), project.manifest());
    Ok(())
}
