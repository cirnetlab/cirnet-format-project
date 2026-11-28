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
fn native_cancelled_publication_clears_the_destination_for_retry() {
    use cirnet_format_project::package::{MANIFEST_ENTRY, verify};

    let (manifest, source) = project();
    let area = tempfile::tempdir().unwrap();
    let destination = area.path().join("destination");
    let bytes = package(&manifest, &source);
    let verified = verify(Cursor::new(&bytes), area.path(), Limits::default()).unwrap();
    let result = verified.publish_with_progress(&destination, |event| {
        if event.member == Some(MANIFEST_ENTRY) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(
        !destination.exists(),
        "cancelled import still claims the destination"
    );
    let project = verify(Cursor::new(&bytes), area.path(), Limits::default())
        .unwrap()
        .publish(&destination)
        .unwrap();
    assert_eq!(project.manifest(), &manifest);
}

#[cfg(feature = "native")]
#[test]
fn interrupted_import_preserves_modified_and_foreign_content() {
    use cirnet_format_project::package::{
        MANIFEST_ENTRY, PackageDestinationState, PackagePublishError, inspect_package_destination,
        verify,
    };
    use std::fs;

    let (manifest, source) = project();
    let area = tempfile::tempdir().unwrap();
    let destination = area.path().join("destination");
    let bytes = package(&manifest, &source);
    let mut verified = verify(Cursor::new(&bytes), area.path(), Limits::default()).unwrap();
    assert_eq!(
        inspect_package_destination(&destination).unwrap(),
        PackageDestinationState::Available
    );

    let modified = destination.join("assets/model.glb");
    let outside = destination.join("operator-notes.txt");
    let result = verified.publish_retryable(&destination, |event| {
        if event.member == Some(MANIFEST_ENTRY) {
            assert_eq!(
                inspect_package_destination(&destination).unwrap(),
                PackageDestinationState::PendingImport
            );
            fs::write(&modified, b"external edit").unwrap();
            fs::write(&outside, b"keep this").unwrap();
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(
        result,
        Err(PackagePublishError::Incomplete { destination: retained, cause, .. })
            if retained.ends_with("destination") && matches!(*cause, Error::Cancelled)
    ));
    assert_eq!(fs::read(&modified).unwrap(), b"external edit");
    assert_eq!(fs::read(&outside).unwrap(), b"keep this");
    assert!(!destination.join(MANIFEST_ENTRY).exists());
    assert_eq!(
        inspect_package_destination(&destination).unwrap(),
        PackageDestinationState::PendingImport
    );
    assert!(matches!(
        verified.publish_retryable(area.path().join("another"), |_| ControlFlow::Continue(())),
        Err(PackagePublishError::Rejected(Error::Validation(_)))
    ));

    let verified = verify(Cursor::new(&bytes), area.path(), Limits::default()).unwrap();
    assert!(matches!(
        verified.publish(&destination),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(&modified).unwrap(), b"external edit");
}

#[cfg(feature = "native")]
#[test]
fn cancellation_in_the_copy_loop_does_not_claim_destination() {
    use cirnet_format_project::package::{
        PackageDestinationState, PackagePublishError, inspect_package_destination, verify,
    };

    let (manifest, source) = project();
    let scratch = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let bytes = package(&manifest, &source);
    let mut verified = verify(Cursor::new(&bytes), scratch.path(), Limits::default()).unwrap();
    let destination = parent.path().join("destination");
    let cancelled = verified.publish_retryable(&destination, |event| {
        if event.member == Some("assets/model.glb") && event.bytes != 0 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(
        cancelled,
        Err(PackagePublishError::Rejected(Error::Cancelled))
    ));
    assert_eq!(
        inspect_package_destination(&destination).unwrap(),
        PackageDestinationState::Available
    );
    let published = verified
        .publish_retryable(&destination, |_| ControlFlow::Continue(()))
        .unwrap();
    assert!(published.warning.is_none());
    assert_eq!(published.project.manifest(), &manifest);
    assert_eq!(
        inspect_package_destination(&destination).unwrap(),
        PackageDestinationState::ManifestPresent
    );
}

#[cfg(feature = "native")]
#[test]
fn abrupt_exit_leaves_an_identifiable_incomplete_import() {
    use cirnet_format_project::package::{
        MANIFEST_ENTRY, PackageDestinationState, inspect_package_destination, verify,
    };
    use std::{path::PathBuf, process::Command};

    const CHILD_PARENT: &str = "CIRNET_TEST_IMPORT_EXIT_PARENT";
    const TEST_NAME: &str = "abrupt_exit_leaves_an_identifiable_incomplete_import";
    const EXIT_AT_MANIFEST: i32 = 71;
    if let Some(parent) = std::env::var_os(CHILD_PARENT) {
        let parent = PathBuf::from(parent);
        let (manifest, source) = project();
        let verified = verify(
            Cursor::new(package(&manifest, &source)),
            &parent,
            Limits::default(),
        )
        .unwrap();
        verified
            .publish_with_progress(parent.join("destination"), |event| {
                if event.member == Some(MANIFEST_ENTRY) {
                    std::process::exit(EXIT_AT_MANIFEST);
                }
                ControlFlow::Continue(())
            })
            .unwrap();
        panic!("publication did not reach the exit checkpoint");
    }

    let area = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST_NAME])
        .env(CHILD_PARENT, area.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(EXIT_AT_MANIFEST),
        "child stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let target = area.path().join("destination");
    assert_eq!(
        inspect_package_destination(&target).unwrap(),
        PackageDestinationState::PendingImport
    );
    assert!(target.join("assets/model.glb").is_file());
    assert!(!target.join(MANIFEST_ENTRY).exists());
}

#[cfg(feature = "native")]
#[test]
fn published_manifest_survives_marker_cleanup_failure() {
    use cirnet_format_project::package::{
        MANIFEST_ENTRY, PackageDestinationState, inspect_package_destination, verify,
    };
    use std::fs;

    let (manifest, source) = project();
    let area = tempfile::tempdir().unwrap();
    let target = area.path().join("destination");
    let bytes = package(&manifest, &source);
    let mut verified = verify(Cursor::new(&bytes), area.path(), Limits::default()).unwrap();
    let marker = target.join(".cirnet-incomplete-import");
    let result = verified
        .publish_retryable(&target, |event| {
            if event.member.is_none() && marker.is_file() {
                fs::write(&marker, b"externally modified marker").unwrap();
            }
            ControlFlow::Continue(())
        })
        .unwrap();
    assert!(matches!(result.warning, Some(Error::PublishedCleanup(_))));
    assert_eq!(result.project.manifest(), &manifest);
    assert!(target.join(MANIFEST_ENTRY).is_file());
    assert_eq!(
        inspect_package_destination(&target).unwrap(),
        PackageDestinationState::ManifestPresent
    );
    assert_eq!(fs::read(marker).unwrap(), b"externally modified marker");
}
