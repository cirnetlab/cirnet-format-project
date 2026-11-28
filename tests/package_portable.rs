#![cfg(feature = "package")]

use cirnet_format_project::{
    AssetEntry, Document, DocumentEntry, Error, Limits, Manifest, ProjectPath, Result,
    package::{
        Compression, PackagePhase, PackageSource, export_from, inspect, verify_only,
        verify_only_with_progress,
    },
};
use std::{collections::BTreeMap, io::Cursor, ops::ControlFlow};

type Resources = BTreeMap<ProjectPath, Vec<u8>>;

struct MemorySource(Resources);

impl PackageSource for MemorySource {
    type Reader<'a> = &'a [u8];

    fn open(&self, path: &ProjectPath) -> Result<Self::Reader<'_>> {
        self.0
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| Error::Missing(path.to_string()))
    }
}

fn project() -> (Manifest, MemorySource) {
    let mut manifest = Manifest::new("lab", "Portable lab", vec!["electronics".into()]);
    let asset = ProjectPath::new("assets/model.glb").unwrap();
    manifest.assets.push(AssetEntry {
        id: "model".into(),
        path: asset.clone(),
        media_type: "model/gltf-binary".into(),
        extensions: Default::default(),
    });
    let doc = ProjectPath::new("documents/drawing.cirsch").unwrap();
    manifest.documents.push(DocumentEntry {
        id: "drawing".into(),
        kind: "schematic".into(),
        schema_version: "owner.1".into(),
        path: doc.clone(),
        title: None,
        links: vec![],
        extensions: Default::default(),
    });
    let bytes = Document::new(
        &manifest.documents[0],
        serde_json::json!({"positions":[1,2]}),
    )
    .to_bytes(&Limits::default())
    .unwrap();
    (
        manifest,
        MemorySource(BTreeMap::from([(asset, vec![23; 200_000]), (doc, bytes)])),
    )
}

fn package(manifest: &Manifest, source: &MemorySource) -> Vec<u8> {
    export_from(
        Cursor::new(Vec::new()),
        manifest,
        source,
        Compression::Deflate,
        Limits::default(),
    )
    .unwrap()
    .into_inner()
}

#[test]
fn package_only_round_trip_from_borrowed_streams() {
    let (manifest, source) = project();
    let limits = Limits::default();
    let bytes = package(&manifest, &source);
    assert_eq!(inspect(Cursor::new(&bytes), limits).unwrap(), manifest);
    assert_eq!(verify_only(Cursor::new(&bytes), limits).unwrap(), manifest);

    let limits = Limits {
        max_asset_bytes: 199_999,
        ..limits
    };
    assert!(matches!(
        verify_only(Cursor::new(&bytes), limits),
        Err(Error::Limit { .. })
    ));
}

#[test]
fn inspect_accepts_declared_members_but_verify_reads_all_payloads() {
    let (manifest, source) = project();
    let limits = Limits::default();
    let mut bytes = package(&manifest, &source);
    let asset_offset = bytes
        .windows(256)
        .position(|window| window.iter().all(|byte| *byte == 23))
        .expect("asset is stored without compression");
    bytes[asset_offset] = 42;
    assert_eq!(inspect(Cursor::new(&bytes), limits).unwrap(), manifest);
    assert!(verify_only(Cursor::new(&bytes), limits).is_err());
}

#[test]
fn cancellation_interrupts_a_member_before_its_final_chunk() {
    let (manifest, source) = project();
    let mut asset_progress = Vec::new();
    let result = verify_only_with_progress(
        Cursor::new(package(&manifest, &source)),
        Limits::default(),
        |event| {
            assert_eq!(event.phase, PackagePhase::Verify);
            if event.member == Some("assets/model.glb") {
                asset_progress.push(event.bytes);
                if event.bytes > 0 {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(asset_progress.first(), Some(&0));
    assert!(asset_progress.last().is_some_and(|bytes| *bytes < 200_000));
}

#[cfg(feature = "native")]
#[test]
fn portable_and_native_export_produce_the_same_bytes() {
    use cirnet_format_project::{package::export, storage::Project};
    use std::fs;

    let (manifest, source) = project();
    let root = tempfile::tempdir().unwrap();
    for (path, data) in &source.0 {
        let destination = root.path().join(path.as_path());
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, data).unwrap();
    }
    let project = Project::create(
        root.path().join("lab.cirproj"),
        manifest.clone(),
        Limits::default(),
    )
    .unwrap();
    let native = export(Cursor::new(Vec::new()), &project, Compression::Deflate)
        .unwrap()
        .into_inner();
    assert_eq!(package(&manifest, &source), native);
}

#[cfg(feature = "native")]
#[test]
fn native_cancelled_verification_cleans_stage() {
    use cirnet_format_project::package::verify_with_progress;
    use std::fs;

    let (manifest, source) = project();
    let scratch = tempfile::tempdir().unwrap();
    let result = verify_with_progress(
        Cursor::new(package(&manifest, &source)),
        scratch.path(),
        Limits::default(),
        |event| {
            if event.member == Some("assets/model.glb") && event.bytes > 0 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(fs::read_dir(scratch.path()).unwrap().next().is_none());
}

#[cfg(feature = "native")]
#[test]
fn native_cancelled_publication_leaves_no_manifest() {
    use cirnet_format_project::package::{MANIFEST_ENTRY, verify};

    let (manifest, source) = project();
    let area = tempfile::tempdir().unwrap();
    let destination = area.path().join("destination");
    let package = verify(
        Cursor::new(package(&manifest, &source)),
        area.path(),
        Limits::default(),
    )
    .unwrap();
    let result = package.publish_with_progress(&destination, |event| {
        if event.member == Some(MANIFEST_ENTRY) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!destination.join(MANIFEST_ENTRY).exists());
}
