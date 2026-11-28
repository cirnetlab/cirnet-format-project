use cirnet_format_project::{Limits, Manifest, ProjectPath, decode_manifest};

#[test]
fn strict_manifest_read_and_portable_paths() {
    assert!(ProjectPath::new("../outside").is_err());
    assert!(ProjectPath::new("data/COM1.txt").is_err());
    let limits = Limits::default();
    let manifest = Manifest::new("project-1", "Laboratory", vec!["electronics".into()]);
    let bytes = manifest.to_bytes(&limits).unwrap();
    assert_eq!(decode_manifest(&bytes, &limits).unwrap(), manifest);
    let duplicate = br#"{"format":"cirnet.project","format":"cirnet.project"}"#;
    assert!(decode_manifest(duplicate, &limits).is_err());
}

use cirnet_format_project::{
    Document, DocumentEntry, DocumentRead, Error, Link, Migration, Relationship, Resource,
    decode_document, import_legacy_circuit, migrate_json,
};
use serde_json::{Value, json};

fn entry(id: &str, kind: &str, path: &str) -> DocumentEntry {
    DocumentEntry {
        id: id.into(),
        kind: kind.into(),
        path: ProjectPath::new(path).unwrap(),
        schema_version: "owner.1".into(),
        title: None,
        links: vec![],
        extensions: Default::default(),
    }
}
fn link(relationship: Relationship, target: &str) -> Link {
    Link {
        relationship,
        target: target.into(),
    }
}

#[test]
fn indexed_references_survive_relocation_and_hard_cycles_fail() {
    let limits = Limits::default();
    let mut project = Manifest::new(
        "p",
        "Project",
        vec!["electronics".into(), "mechanical".into()],
    );
    project.documents = vec![
        entry("sch", "schematic", "drawing/main.cirsch"),
        entry("pcb", "pcb", "board.cirpcb"),
        entry("cad", "assembly", "assembly.cir3d"),
        entry("lib", "library", "library.cirlib"),
    ];
    project.documents[0].links = vec![
        link(Relationship::Reference, "pcb"),
        link(Relationship::Library, "lib"),
    ];
    project.documents[1].links = vec![link(Relationship::Reference, "sch")];
    project.links = vec![
        link(Relationship::Configuration, "sch"),
        link(Relationship::Result, "pcb"),
    ];
    project.documents[1].path = ProjectPath::new("relocated/new.cirpcb").unwrap();
    let index = project.index(&limits).unwrap();
    assert!(
        matches!(index.resolve(&project.documents[0].links[0]).unwrap(), Resource::Document(item) if item.path.as_str() == "relocated/new.cirpcb")
    );
    for entry in &project.documents {
        let bytes = Document::new(entry, json!({"owner":"pending"}))
            .to_bytes(&limits)
            .unwrap();
        assert!(matches!(
            decode_document(entry, &bytes, &limits).unwrap(),
            DocumentRead::Envelope(_)
        ));
    }
    project.documents[0]
        .links
        .push(link(Relationship::Dependency, "pcb"));
    project.documents[1]
        .links
        .push(link(Relationship::Dependency, "sch"));
    assert!(
        matches!(project.index(&limits), Err(Error::Validation(reason)) if reason.contains("cycle"))
    );
    project.documents[1].links.clear();
    project.documents[0]
        .links
        .push(link(Relationship::Reference, "missing"));
    assert!(matches!(project.index(&limits), Err(Error::Missing(_))));
}

#[test]
fn strict_json_rejects_ambiguous_or_excessive_input_and_preserves_numbers() {
    let limits = Limits::default();
    assert!(matches!(
        decode_manifest(include_bytes!("fixtures/duplicate-key.json"), &limits),
        Err(Error::JsonPolicy {
            reason: "duplicate object key",
            ..
        })
    ));
    assert!(matches!(
        decode_manifest(b"\xFF", &limits),
        Err(Error::JsonPolicy { .. })
    ));
    let limited = Limits {
        max_json_depth: 2,
        ..limits
    };
    assert!(matches!(
        decode_manifest(br#"{"a":[[1]]}"#, &limited),
        Err(Error::Limit {
            resource: "JSON depth",
            ..
        })
    ));
    let limited = Limits {
        max_json_bytes: 16,
        ..limits
    };
    assert!(matches!(
        decode_manifest(&[b' '; 17], &limited),
        Err(Error::Limit { .. })
    ));
    let doc = entry("sch", "schematic", "board.cirsch");
    let precision: Value =
        serde_json::from_str(r#"{"big":9007199254740993,"fraction":0.123456789012345678901}"#)
            .unwrap();
    let bytes = Document::new(&doc, precision.clone())
        .to_bytes(&limits)
        .unwrap();
    assert!(
        matches!(decode_document(&doc, &bytes, &limits).unwrap(), DocumentRead::Envelope(doc) if doc.payload == precision)
    );
    for number in ["1e309", "1e-400", "NaN", "01"] {
        let text = format!(r#"{{"payload":{number}}}"#);
        assert!(decode_document(&doc, text.as_bytes(), &limits).is_err());
    }
    assert!(decode_document(&doc, br#"{"refresh\u005ftoken":""}"#, &limits).is_err());
}

#[test]
fn manifest_collisions_and_opaque_documents_have_distinct_diagnostics() {
    let limits = Limits::default();
    for path in [
        "../x",
        "/x",
        "D:x",
        "data\\x",
        "data/.env",
        "x. ",
        "LPT1.bin",
        "safe//file",
        "data/x\0",
    ] {
        assert!(ProjectPath::new(path).is_err(), "{path:?}");
    }
    let mut project = Manifest::new("project", "Project", vec!["electronics".into()]);
    project.documents = vec![
        entry("one", "schematic", "Design/a.cirsch"),
        entry("two", "pcb", "design/b.cirpcb"),
    ];
    assert!(matches!(project.index(&limits), Err(Error::Conflict(_))));
    project.documents[1].path = ProjectPath::new("other/b.cirpcb").unwrap();
    project.documents[1].id = "one".into();
    assert!(
        matches!(project.index(&limits), Err(Error::Validation(reason)) if reason.contains("duplicate"))
    );
    let unknown = entry("probe", "owner.future", "probe.cirx");
    let bytes = Document::new(&unknown, json!({"vendor":9}))
        .to_bytes(&limits)
        .unwrap();
    assert_eq!(
        decode_document(&unknown, &bytes, &limits).unwrap(),
        DocumentRead::Opaque(&bytes)
    );
    let disallowed =
        br#"{"format":"cirnet.project","format_version":"2.0","future":{"field":true}}"#;
    assert!(matches!(
        decode_manifest(disallowed, &limits),
        Err(Error::Unsupported {
            domain: "project",
            ..
        })
    ));
}

#[test]
fn owner_migrations_require_an_explicit_exact_transition() {
    let limits = Limits::default();
    let source = br#"{"format":"example","format_version":"1.0","data":9}"#;
    assert!(matches!(
        migrate_json(source, "example", "2.0", &[], &limits),
        Err(Error::Unsupported { .. })
    ));
    let step = Migration {
        from: "1.0",
        to: "2.0",
        apply: |mut value| {
            value["format_version"] = json!("2.0");
            Ok(value)
        },
    };
    let bytes = migrate_json(source, "example", "2.0", &[step], &limits).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["data"], 9);
}

#[test]
fn legacy_import_keeps_all_input_bytes_and_requires_validation() {
    let limits = Limits::default();
    for (version, original) in [
        (
            "1.0",
            include_bytes!("fixtures/legacy-v1.0.json").as_slice(),
        ),
        (
            "1.1",
            include_bytes!("fixtures/legacy-v1.1.json").as_slice(),
        ),
        (
            "1.2",
            include_bytes!("fixtures/legacy-v1.2.json").as_slice(),
        ),
    ] {
        let path = ProjectPath::new("circuits/imported.json").unwrap();
        let (manifest, bytes) = import_legacy_circuit(
            original,
            "lab",
            "Imported lab",
            "graph",
            path.clone(),
            &limits,
            |actual, supplied| {
                assert_eq!(actual, version);
                assert_eq!(supplied, original);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(bytes, original);
        assert_eq!(manifest.documents[0].schema_version, version);
        assert_eq!(
            decode_document(&manifest.documents[0], bytes, &limits).unwrap(),
            DocumentRead::LegacyCircuit(bytes)
        );
        assert!(
            import_legacy_circuit(
                original,
                "lab",
                "Imported lab",
                "graph",
                path,
                &limits,
                |_, _| Err("owner rejected input".into())
            )
            .is_err()
        );
    }
}

#[test]
fn manifest_fast_path_retains_header_and_strict_input_diagnostics() {
    let limits = Limits::default();
    let future =
        br#"{"format":"cirnet.project","format_version":"2.0","unknown_future_field":true}"#;
    assert!(matches!(
        decode_manifest(future, &limits),
        Err(Error::Unsupported { domain: "project", version }) if version == "2.0"
    ));
    let malformed_header = br#"{"format":"cirnet.project","format_version":1,"name":"Lab"}"#;
    assert!(matches!(
        decode_manifest(malformed_header, &limits),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        decode_manifest(
            br#"["cirnet.project","1.0","lab","Lab",["electronics"]]"#,
            &limits
        ),
        Err(Error::Validation(_))
    ));
    let duplicate = br#"{"format":"cirnet.project","format_version":"1.0","format_version":"2.0"}"#;
    assert!(matches!(
        decode_manifest(duplicate, &limits),
        Err(Error::JsonPolicy { .. })
    ));
}
