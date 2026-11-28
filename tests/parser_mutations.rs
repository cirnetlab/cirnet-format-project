use cirnet_format_project::{
    Document, DocumentEntry, DocumentRead, Error, Limits, Manifest, Migration, ProjectPath,
    decode_document, decode_manifest, detect_version, migrate_json,
};
use serde_json::{Value, json};

#[cfg(feature = "package")]
#[path = "support/archive.rs"]
mod archive;

const SEED: u32 = 0x24bc_19f3;
const ROUNDS: usize = 128;

fn mutations(label: &str, bytes: &[u8], rounds: usize, mut test: impl FnMut(&[u8])) {
    assert!(!bytes.is_empty());
    let mut seed = SEED;
    test(bytes);
    for multiplier in [1, 2, 4, 8] {
        for _ in 0..rounds {
            let mut candidate = bytes.to_vec();
            for _ in 0..multiplier {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let pos = seed as usize % candidate.len();
                candidate[pos] ^= ((seed >> 16) as u8).max(1);
            }
            test(&candidate);
        }
    }
    for end in 0..bytes.len() {
        test(&bytes[..end]);
    }
    eprintln!("{label}: {} inputs", 1 + 4 * rounds + bytes.len());
}

#[test]
fn bounded_json_mutations_preserve_canonical_round_trip_if_accepted() {
    let limits = Limits::default();
    let manifest = Manifest::new("mutations", "Mutations", vec!["electronics".into()]);
    let seed = manifest.to_bytes(&limits).unwrap();
    mutations("manifest", &seed, ROUNDS, |input| {
        if let Ok(parsed) = decode_manifest(input, &limits) {
            assert_eq!(
                decode_manifest(&parsed.to_bytes(&limits).unwrap(), &limits).unwrap(),
                parsed
            );
        }
    });
}

#[cfg(feature = "package")]
#[test]
fn bounded_archive_mutations_keep_full_verification_at_least_as_strict_as_inspect() {
    use cirnet_format_project::{
        ProjectPath, Result,
        package::{Compression, PackageSource, export_from, inspect, verify_only},
    };
    use std::io::Cursor;
    struct EmptyProject;
    impl PackageSource for EmptyProject {
        type Reader<'a> = Cursor<&'a [u8]>;
        fn open(&self, path: &ProjectPath) -> Result<Self::Reader<'_>> {
            Err(cirnet_format_project::Error::Missing(path.to_string()))
        }
    }
    let limits = Limits::default();
    let manifest = Manifest::new("mutations", "Mutations", vec!["electronics".into()]);
    let archive = export_from(
        Cursor::new(Vec::new()),
        &manifest,
        &EmptyProject,
        Compression::Stored,
        limits,
    )
    .unwrap()
    .into_inner();
    mutations(
        "ZIP preflight and verification",
        &archive,
        ROUNDS,
        |input| {
            if let Ok(parsed) = verify_only(Cursor::new(input), limits) {
                assert_eq!(inspect(Cursor::new(input), limits).unwrap(), parsed);
            }
        },
    );
}

fn entry(kind: &str, version: &str, path: &str) -> DocumentEntry {
    DocumentEntry {
        id: "drawing".into(),
        kind: kind.into(),
        schema_version: version.into(),
        path: ProjectPath::new(path).unwrap(),
        title: None,
        links: vec![],
        extensions: Default::default(),
    }
}

#[test]
fn equivalent_key_spellings_are_duplicates_and_large_payload_numbers_are_exact() {
    let limits = Limits::default();
    let entry = entry("schematic", "test.1", "drawing.cirsch");
    for key in b'a'..=b'z' {
        let escaped = format!(r"\u{key:04x}");
        let raw = format!(
            r#"{{"format":"cirnet.document","format_version":"1.0","id":"drawing","kind":"schematic","schema_version":"test.1","payload":{{"{}":0,"{escaped}":1}}}}"#,
            char::from(key),
        );
        assert!(matches!(
            decode_document(&entry, raw.as_bytes(), &limits),
            Err(Error::JsonPolicy {
                reason: "duplicate object key",
                ..
            })
        ));
    }
    for number in 9_007_199_254_740_992_u64..9_007_199_254_741_024_u64 {
        let raw = format!(r#"{{"count":{number},"fraction":0.123456789012345678901}}"#);
        let payload: Value = serde_json::from_str(&raw).unwrap();
        let bytes = Document::new(&entry, payload.clone())
            .to_bytes(&limits)
            .unwrap();
        let DocumentRead::Envelope(decoded) = decode_document(&entry, &bytes, &limits).unwrap()
        else {
            panic!("expected recognized envelope");
        };
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.payload["count"].as_u64(), Some(number));
        assert_eq!(
            decoded.payload["fraction"].to_string(),
            "0.123456789012345678901"
        );
    }
    eprintln!("escaped duplicate keys and precise payload numbers: 58 inputs");
}

#[test]
fn document_mutations_never_lose_accepted_envelope_or_opaque_content() {
    let limits = Limits::default();
    let ordinary = entry("schematic", "test.1", "drawing.cirsch");
    let bytes = Document::new(&ordinary, json!({"positions":[1,2],"value":"µ"}))
        .to_bytes(&limits)
        .unwrap();
    let historical = entry("circuit-graph", "1.2", "drawing.json");
    for (label, entry, seed) in [
        ("document envelope", ordinary, bytes.as_slice()),
        (
            "legacy CircuitGraph",
            historical,
            include_bytes!("fixtures/legacy-v1.2.json").as_slice(),
        ),
    ] {
        mutations(label, seed, ROUNDS, |input| {
            match decode_document(&entry, input, &limits) {
                Ok(DocumentRead::Envelope(document)) => {
                    let bytes = document.to_bytes(&limits).unwrap();
                    assert_eq!(
                        decode_document(&entry, &bytes, &limits).unwrap(),
                        DocumentRead::Envelope(document)
                    );
                }
                Ok(DocumentRead::Opaque(original) | DocumentRead::LegacyCircuit(original)) => {
                    assert_eq!(original, input);
                }
                Err(_) => {}
            }
        });
    }
}

#[cfg(feature = "package")]
#[test]
fn inventory_mutations_reach_integrity_parser_with_valid_archive_records() {
    use cirnet_format_project::{
        Digest,
        package::{inspect, verify_only},
    };

    let limits = Limits::default();
    let manifest = Manifest::new("mutations", "Inventory", vec!["electronics".into()]);
    let manifest_bytes = manifest.to_bytes(&limits).unwrap();
    let raw = serde_json::to_vec(&json!({
        "format": "cirnet.package",
        "format_version": "1.0",
        "algorithm": "sha256",
        "entries": {"project.cirproj": {
            "bytes": manifest_bytes.len(),
            "sha256": Digest::of(&manifest_bytes),
        }}
    }))
    .unwrap();
    assert_eq!(
        verify_only(archive::with_inventory(&manifest_bytes, &raw), limits).unwrap(),
        manifest
    );
    mutations("integrity inventory", &raw, ROUNDS, |input| {
        let candidate = archive::with_inventory(&manifest_bytes, input);
        if let Ok(parsed) = verify_only(candidate.clone(), limits) {
            assert_eq!(parsed, manifest);
            assert_eq!(inspect(candidate, limits).unwrap(), parsed);
        }
    });
}

#[cfg(feature = "native")]
#[test]
fn recovery_journal_mutations_preserve_public_files_and_uncertain_evidence() {
    use cirnet_format_project::{Digest, storage::Project, transaction::RecoveryState};
    use std::fs;

    let area = tempfile::tempdir().unwrap();
    let anchor = area.path().join("Lab.cirproj");
    let stage = area.path().join(".cirnet-transaction");
    let journal = stage.join("journal.json");
    let sentinel = area.path().join("existing.bin");
    fs::write(&sentinel, b"external contents").unwrap();

    let manifest = Manifest::new("mutations", "Recovery", vec!["electronics".into()]);
    let project = Project::create(&anchor, manifest, Limits::default()).unwrap();
    let original = fs::read(&anchor).unwrap();
    let raw = serde_json::to_vec(&json!({
        "version": 1,
        "manifest": "Lab.cirproj",
        "before": project.revision(),
        "after": Digest::of(b"next manifest"),
        "files": [{"path": "existing.bin", "digest": Digest::of(b"draft"), "bytes": 5}],
    }))
    .unwrap();
    mutations("recovery journal", &raw, 16, |input| {
        fs::create_dir_all(&stage).unwrap();
        fs::write(&journal, input).unwrap();
        match project.inspect_recovery() {
            Ok(RecoveryState::BeforePublication | RecoveryState::Published) => {
                let initial = project.inspect_recovery().unwrap();
                assert_eq!(project.recover().unwrap().state, initial);
                assert_eq!(project.inspect_recovery().unwrap(), RecoveryState::Clean);
            }
            Ok(RecoveryState::Diverged) => {
                assert_eq!(project.recover().unwrap().state, RecoveryState::Diverged);
                assert_eq!(fs::read(&journal).unwrap(), input);
            }
            Err(_) => {
                assert!(project.recover().is_err());
                assert_eq!(fs::read(&journal).unwrap(), input);
            }
            Ok(other) => panic!("unexpected state {other:?} for an existing journal"),
        }
        assert_eq!(fs::read(&anchor).unwrap(), original);
        assert_eq!(fs::read(&sentinel).unwrap(), b"external contents");
    });
}

#[test]
fn migration_selection_mutations_reach_only_the_explicit_target() {
    let limits = Limits::default();
    let source = br#"{"format":"test.migration","format_version":"1.0","value":123}"#;
    let step = Migration {
        from: "1.0",
        to: "2.0",
        apply: |mut value| {
            value["format_version"] = json!("2.0");
            Ok(value)
        },
    };
    let steps = [step];
    assert!(migrate_json(source, "test.migration", "2.0", &steps, &limits).is_ok());
    mutations("migration selection", source, ROUNDS, |input| {
        if let Ok(output) = migrate_json(input, "test.migration", "2.0", &steps, &limits) {
            assert!(matches!(
                detect_version(input, &limits).unwrap().1.as_str(),
                "1.0" | "2.0"
            ));
            assert_eq!(
                detect_version(&output, &limits).unwrap(),
                ("test.migration".into(), "2.0".into())
            );
            assert_eq!(
                migrate_json(&output, "test.migration", "2.0", &[], &limits).unwrap(),
                output
            );
        }
    });
}
