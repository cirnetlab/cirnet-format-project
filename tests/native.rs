#![cfg(feature = "native")]
use cirnet_format_project::package::{Compression, export, verify};
use cirnet_format_project::storage::{Project, StagedFile};
use cirnet_format_project::transaction::{ChangeSet, ResourceWrite, WriteMode};
use cirnet_format_project::{
    AssetEntry, Digest, Document, DocumentEntry, Error, Limits, Manifest, ProjectPath,
};
use cirnet_format_project::{
    CodecKey, CodecRegistration, CodecRegistry, DocumentKind, EnvelopeVersion,
};
use std::{
    fs,
    io::{Cursor, Read},
};

fn fixture_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    registry
        .register(CodecRegistration {
            key: CodecKey {
                kind: DocumentKind::Schematic,
                envelope_version: EnvelopeVersion::V1,
                schema_version: "test.1".into(),
            },
            validate: |_, _| Ok(()),
            writable: true,
            migratable: false,
        })
        .unwrap();
    registry
}

fn create_fixture(project: &mut Project, manifest: Manifest, document: &[u8], asset: &[u8]) {
    let mut change = ChangeSet::new(project.revision().clone());
    change.write(ResourceWrite::new(
        &manifest.documents[0].id,
        manifest.documents[0].path.clone(),
        WriteMode::Create,
        document,
    ));
    change.write(ResourceWrite::new(
        &manifest.assets[0].id,
        manifest.assets[0].path.clone(),
        WriteMode::Create,
        asset,
    ));
    change.set_manifest(manifest);
    project.apply(change, &fixture_registry()).unwrap();
}

fn replace_asset(
    project: &mut Project,
    next: Manifest,
    reader: impl Read,
) -> cirnet_format_project::Result<()> {
    let id = &next.assets[0].id;
    let mut change = ChangeSet::new(project.revision().clone());
    change.expect(id, project.resource_revision(id)?);
    change.write(ResourceWrite::new(
        id,
        next.assets[0].path.clone(),
        WriteMode::Replace,
        reader,
    ));
    change.set_manifest(next);
    project.apply(change, &fixture_registry())
}

fn sample() -> Manifest {
    let mut manifest = Manifest::new("lab-1", "Portable lab", vec!["electronics".into()]);
    manifest.documents.push(DocumentEntry {
        id: "drawing".into(),
        kind: "schematic".into(),
        schema_version: "test.1".into(),
        path: ProjectPath::new("documents/drawing.cirsch").unwrap(),
        title: Some("Drawing".into()),
        links: vec![],
        extensions: Default::default(),
    });
    manifest.assets.push(AssetEntry {
        id: "model".into(),
        path: ProjectPath::new("assets/model.glb").unwrap(),
        media_type: "model/gltf-binary".into(),
        extensions: Default::default(),
    });
    manifest
}

#[test]
fn staged_commit_and_verified_package_round_trip() {
    let area = tempfile::tempdir().unwrap();
    let root = area.path().join("local");
    fs::create_dir(&root).unwrap();
    let limits = Limits::default();
    let initial = Manifest::new("lab-1", "Portable lab", vec!["electronics".into()]);
    let mut project = Project::create(root.join("lab.cirproj"), initial, limits).unwrap();
    let manifest = sample();
    let doc = &manifest.documents[0];
    let bytes = Document::new(doc, serde_json::json!({"positions":[1,2,3]}))
        .to_bytes(&limits)
        .unwrap();
    let binary = vec![23; 131_072];
    let stale = Project::open(root.join("lab.cirproj"), limits).unwrap();
    create_fixture(&mut project, manifest.clone(), &bytes, &binary);
    let mut stale = stale;
    assert!(matches!(
        stale.commit(stale.manifest().clone(), Vec::<StagedFile<&[u8]>>::new()),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        project.read_document("drawing").unwrap().revision,
        Digest::of(&bytes)
    );
    let archive = export(Cursor::new(Vec::new()), &project, Compression::Deflate)
        .unwrap()
        .into_inner();
    let repeated = export(Cursor::new(Vec::new()), &project, Compression::Deflate)
        .unwrap()
        .into_inner();
    assert_eq!(archive, repeated);
    let verified = verify(Cursor::new(archive), area.path(), limits).unwrap();
    assert_eq!(verified.manifest(), &manifest);
    let imported = verified.publish(area.path().join("imported")).unwrap();
    assert_eq!(
        fs::read(imported.root().join("assets/model.glb")).unwrap(),
        binary
    );
    assert_eq!(imported.read_document("drawing").unwrap().bytes, bytes);
}

fn rewrite_archive(
    archive: &[u8],
    mut transform: impl FnMut(&str, &mut Vec<u8>),
    extra: Option<&str>,
) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut original = zip::ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut result = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for index in 0..original.len() {
        let mut entry = original.by_index(index).unwrap();
        let name = std::str::from_utf8(entry.name_raw()).unwrap().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        transform(&name, &mut bytes);
        result.start_file(name, options).unwrap();
        result.write_all(&bytes).unwrap();
    }
    if let Some(extra) = extra {
        result.start_file(extra, options).unwrap();
    }
    result.finish().unwrap().into_inner()
}

#[test]
fn verified_import_rejects_corruption_and_never_replaces_existing_destination() {
    let area = tempfile::tempdir().unwrap();
    let folder = area.path().join("source");
    fs::create_dir(&folder).unwrap();
    let limits = Limits::default();
    let manifest = Manifest::new("p", "Project", vec!["electronics".into()]);
    let project = Project::create(folder.join("sample.cirproj"), manifest, limits).unwrap();
    let bytes = export(Cursor::new(Vec::new()), &project, Compression::Stored)
        .unwrap()
        .into_inner();

    let bad_digest = rewrite_archive(
        &bytes,
        |name, payload| {
            if name == "project.cirproj" {
                let pos = payload
                    .windows(7)
                    .position(|text| text == b"Project")
                    .unwrap();
                payload[pos] = b'p';
            }
        },
        None,
    );
    assert!(matches!(
        verify(Cursor::new(bad_digest), area.path(), limits),
        Err(Error::Integrity(_))
    ));
    assert!(verify(Cursor::new(&bytes[..bytes.len() - 1]), area.path(), limits).is_err());
    let limited = Limits {
        max_archive_bytes: bytes.len() as u64 - 1,
        ..limits
    };
    assert!(matches!(
        verify(Cursor::new(&bytes), area.path(), limited),
        Err(Error::Limit { .. })
    ));
    let limited = Limits {
        max_directory_bytes: 50,
        ..limits
    };
    assert!(matches!(
        verify(Cursor::new(&bytes), area.path(), limited),
        Err(Error::Limit { .. })
    ));
    let case_alias = rewrite_archive(&bytes, |_, _| (), Some("PROJECT.CIRPROJ"));
    assert!(matches!(
        verify(Cursor::new(case_alias), area.path(), limits),
        Err(Error::Conflict(_))
    ));
    let traversal = rewrite_archive(&bytes, |_, _| (), Some("safe/../outside.json"));
    assert!(matches!(
        verify(Cursor::new(traversal), area.path(), limits),
        Err(Error::Path(_))
    ));
    let extra = rewrite_archive(&bytes, |_, _| (), Some("unregistered.txt"));
    assert!(matches!(
        verify(Cursor::new(extra), area.path(), limits),
        Err(Error::Integrity(_))
    ));
    assert!(matches!(
        verify(Cursor::new(&bytes), area.path(), limits)
            .unwrap()
            .publish(&folder),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        Project::open(folder.join("sample.cirproj"), limits)
            .unwrap()
            .revision(),
        project.revision()
    );
}

#[test]
fn failed_document_validation_and_interrupted_commit_leave_the_previous_manifest() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("lab.cirproj");
    let limits = Limits::default();
    let mut project = Project::create(
        &path,
        Manifest::new("lab-1", "Lab", vec!["electronics".into()]),
        limits,
    )
    .unwrap();
    let manifest = sample();
    let doc = &manifest.documents[0];
    let bytes = Document::new(doc, serde_json::json!({"record":1}))
        .to_bytes(&limits)
        .unwrap();
    create_fixture(&mut project, manifest.clone(), &bytes, b"model");
    let original = fs::read(&path).unwrap();
    let revision = Digest::of(&bytes);
    let changed = Document::new(doc, serde_json::json!({"record":2}))
        .to_bytes(&limits)
        .unwrap();
    assert!(
        project
            .save_document("drawing", &revision, &changed, |_, _| Err(
                "owner rejection".into()
            ))
            .is_err()
    );
    assert_eq!(project.read_document("drawing").unwrap().bytes, bytes);
    let revision2 = project
        .save_document("drawing", &revision, &changed, |_, _| Ok(()))
        .unwrap();
    assert_ne!(revision, revision2);
    assert!(matches!(
        project.save_document("drawing", &revision, &bytes, |_, _| Ok(())),
        Err(Error::Conflict(_))
    ));

    struct Interrupt(std::path::PathBuf);
    impl std::io::Read for Interrupt {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            if self.0.exists() {
                return Ok(0);
            }
            fs::write(&self.0, "other writer")?;
            Ok(0)
        }
    }

    let mut next = manifest.clone();
    next.assets[0].path = ProjectPath::new("assets/new.glb").unwrap();
    let collision = workspace.path().join("assets/new.glb");
    assert!(matches!(
        replace_asset(&mut project, next.clone(), Interrupt(collision.clone())),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(collision).unwrap(), b"other writer");
    assert_eq!(Project::open(&path, limits).unwrap().manifest(), &manifest);

    next.assets[0].path = ProjectPath::new("assets/renamed.glb").unwrap();
    project.recover().unwrap();
    replace_asset(&mut project, next.clone(), &b"new"[..]).unwrap();
    assert_eq!(Project::open(&path, limits).unwrap().manifest(), &next);
    assert_eq!(
        fs::read(workspace.path().join("assets/model.glb")).unwrap(),
        b"model"
    );
}

#[test]
fn unknown_document_bytes_can_be_packaged_but_not_replaced_lossily() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("lab.cirproj");
    let limits = Limits::default();
    let mut manifest = sample();
    manifest.assets.clear();
    let entry = &manifest.documents[0];
    let original = br#"{"format":"cirnet.document","format_version":"2.0","id":"drawing","next_schema":{"retain":true}}"#;
    fs::create_dir(workspace.path().join("documents")).unwrap();
    fs::write(workspace.path().join(entry.path.as_path()), original).unwrap();
    let changed = Document::new(entry, serde_json::Value::Null)
        .to_bytes(&limits)
        .unwrap();
    let project = Project::create(path, manifest, limits).unwrap();
    let saved = project.read_document("drawing").unwrap();
    assert!(matches!(
        project.save_document("drawing", &saved.revision, &changed, |_, _| Ok(())),
        Err(Error::Unsupported { .. })
    ));
    let bytes = export(Cursor::new(Vec::new()), &project, Compression::Stored)
        .unwrap()
        .into_inner();
    let copy = verify(Cursor::new(bytes), workspace.path(), limits)
        .unwrap()
        .publish(workspace.path().join("copy"))
        .unwrap();
    assert_eq!(copy.read_document("drawing").unwrap().bytes, original);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows must allow symbolic link or junction creation"
)]
fn symlinked_native_resources_are_rejected() {
    let area = tempfile::tempdir().unwrap();
    let root = area.path().join("lab");
    let external = area.path().join("external");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&external).unwrap();
    fs::write(external.join("model.glb"), b"outside").unwrap();
    let assets = root.join("assets");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, &assets).unwrap();
    #[cfg(windows)]
    match std::os::windows::fs::symlink_dir(&external, &assets) {
        Ok(()) => (),
        Err(error) if error.raw_os_error() == Some(1314) => {
            let status = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&assets)
                .arg(&external)
                .status()
                .unwrap();
            assert!(status.success(), "cannot create a fixture junction");
        }
        Err(error) => panic!("{error}"),
    }
    #[cfg(any(unix, windows))]
    {
        let mut manifest = sample();
        manifest.documents.clear();
        assert!(matches!(
            Project::create(root.join("lab.cirproj"), manifest, Limits::default()),
            Err(Error::Path(_))
        ));
        assert!(!root.join("lab.cirproj").exists());
    }
}

// An inspect result can be used for a file chooser, but does not validate asset content.
#[test]
fn fast_package_inspection_is_distinct_from_verification() {
    let area = tempfile::tempdir().unwrap();
    let source = area.path().join("work");
    fs::create_dir(&source).unwrap();
    let limits = Limits::default();
    let mut manifest = sample();
    manifest.documents.clear();
    fs::create_dir(source.join("assets")).unwrap();
    fs::write(source.join("assets/model.glb"), b"model").unwrap();
    let project = Project::create(source.join("lab.cirproj"), manifest.clone(), limits).unwrap();
    let original = export(Cursor::new(Vec::new()), &project, Compression::Stored)
        .unwrap()
        .into_inner();
    let tampered = rewrite_archive(
        &original,
        |name, payload| {
            if name == "assets/model.glb" {
                payload[0] = b'M';
            }
        },
        None,
    );
    assert_eq!(
        cirnet_format_project::package::inspect(Cursor::new(&tampered), limits).unwrap(),
        manifest
    );
    assert!(matches!(
        verify(Cursor::new(tampered), area.path(), limits),
        Err(Error::Integrity(_))
    ));

    let mut project = project;
    let mut next = manifest;
    next.libraries.push(cirnet_format_project::ExternalLibrary {
        id: "external".into(),
        coordinate: "owner.parts".into(),
        version: "1.0".into(),
        required: true,
    });
    project
        .commit(next, Vec::<StagedFile<&[u8]>>::new())
        .unwrap();
    assert!(matches!(
        export(Cursor::new(Vec::new()), &project, Compression::Stored),
        Err(Error::Missing(_))
    ));
}

#[test]
fn zip_rejects_unreported_directory_members_and_conflicting_local_names() {
    let area = tempfile::tempdir().unwrap();
    let limits = Limits::default();
    let project = Project::create(
        area.path().join("lab.cirproj"),
        Manifest::new("p", "Package profile", vec!["electronics".into()]),
        limits,
    )
    .unwrap();
    let original = export(Cursor::new(Vec::new()), &project, Compression::Stored)
        .unwrap()
        .into_inner();

    let mut extra = rewrite_archive(&original, |_, _| (), Some("undeclared.txt"));
    let end = extra.len() - 22;
    extra[end + 8..end + 10].copy_from_slice(&2u16.to_le_bytes());
    extra[end + 10..end + 12].copy_from_slice(&2u16.to_le_bytes());
    assert!(matches!(
        verify(Cursor::new(extra), area.path(), limits),
        Err(Error::Package(_))
    ));

    let mut conflicting = original;
    assert_eq!(&conflicting[..4], b"PK\x03\x04");
    conflicting[30] = b'x';
    assert!(matches!(
        verify(Cursor::new(conflicting), area.path(), limits),
        Err(Error::Package(_))
    ));
}
