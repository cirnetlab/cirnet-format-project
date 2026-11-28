use cirnet_format_project::{
    CodecKey, CodecRegistration, CodecRegistry, Document, DocumentEntry, DocumentKind,
    DocumentRead, EnvelopeVersion, Error, Limits, Link, Manifest, ProjectPath, Relationship,
    ResourceOwner, decode_document, validate_document_transition,
};
use serde_json::json;

fn entry(id: &str, kind: &str, path: &str, schema_version: &str) -> DocumentEntry {
    DocumentEntry {
        id: id.into(),
        kind: kind.into(),
        path: ProjectPath::new(path).unwrap(),
        schema_version: schema_version.into(),
        title: None,
        links: Vec::new(),
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
fn domain_write_requires_an_exact_owner_registration() {
    let limits = Limits::default();
    let entry = entry("example", "schematic", "example.cirsch", "owner.1");
    let bytes = Document::new(&entry, json!({"approved": true}))
        .to_bytes(&limits)
        .unwrap();
    let mut registry = CodecRegistry::new();
    let initial = registry.inspect(&entry, &bytes, &limits).unwrap();
    assert!(initial.capabilities.structural_read);
    assert!(initial.capabilities.byte_preservation);
    assert!(!initial.capabilities.domain_support);
    assert!(!initial.capabilities.writable);
    assert!(!initial.capabilities.migratable);
    assert_eq!(initial.original_bytes(), bytes);
    assert!(matches!(
        registry.validate_write(&entry, &bytes, &limits),
        Err(Error::Unsupported { .. })
    ));

    let registration = CodecRegistration {
        key: CodecKey {
            kind: DocumentKind::Schematic,
            envelope_version: EnvelopeVersion::V1,
            schema_version: "owner.1".into(),
        },
        validate: |_, source| {
            let data: serde_json::Value = serde_json::from_slice(source)?;
            if data["payload"]["approved"] == true {
                Ok(())
            } else {
                Err(Error::Validation("owner rejected payload".into()))
            }
        },
        writable: true,
        migratable: false,
    };
    registry.register(registration.clone()).unwrap();
    assert!(registry.register(registration).is_err());
    let inspected = registry.inspect(&entry, &bytes, &limits).unwrap();
    assert!(inspected.capabilities.domain_support);
    assert!(inspected.capabilities.writable);
    assert!(!inspected.capabilities.migratable);
    registry.validate_write(&entry, &bytes, &limits).unwrap();

    let rejected = Document::new(&entry, json!({"approved": false}))
        .to_bytes(&limits)
        .unwrap();
    assert!(matches!(
        registry.validate_write(&entry, &rejected, &limits),
        Err(Error::Validation(_))
    ));
    let future = br#"{"format":"cirnet.document","format_version":"2.0","id":"example","kind":"schematic","schema_version":"owner.1","payload":{"approved":true}}"#;
    let inspected = registry.inspect(&entry, future, &limits).unwrap();
    assert_eq!(inspected.document, DocumentRead::Opaque(future));
    assert_eq!(inspected.original_bytes(), future);
    assert!(!inspected.capabilities.structural_read);
    assert!(!inspected.capabilities.writable);

    let mut unknown_schema = entry.clone();
    unknown_schema.schema_version = "owner.2".into();
    let bytes = Document::new(&unknown_schema, json!({"approved": true}))
        .to_bytes(&limits)
        .unwrap();
    assert!(
        !registry
            .inspect(&unknown_schema, &bytes, &limits)
            .unwrap()
            .capabilities
            .domain_support
    );
}

#[test]
fn protected_document_transitions_preserve_extensions_and_opaque_bytes() {
    let limits = Limits::default();
    let entry = entry("example", "schematic", "example.cirsch", "owner.1");
    let mut document = Document::new(&entry, json!({"old": true}));
    document
        .extensions
        .insert("owner.extra".into(), json!({"precision": "retained"}));
    let old_bytes = document.to_bytes(&limits).unwrap();
    let before = decode_document(&entry, &old_bytes, &limits).unwrap();

    document.schema_version = "owner.2".into();
    document.payload = json!({"updated": true});
    let after = DocumentRead::Envelope(document.clone());
    validate_document_transition(&entry, &before, &after).unwrap();
    document.extensions.clear();
    assert!(matches!(
        validate_document_transition(&entry, &before, &DocumentRead::Envelope(document)),
        Err(Error::Validation(_))
    ));

    let opaque = DocumentRead::Opaque(br#"{"format":"future","format_version":"2.0"}"#);
    assert!(matches!(
        validate_document_transition(&entry, &opaque, &after),
        Err(Error::Unsupported { .. })
    ));
    validate_document_transition(&entry, &opaque, &opaque).unwrap();
}

#[test]
fn migration_requires_destination_write_and_migration_capabilities() {
    let limits = Limits::default();
    let entry = entry("model", "owner.model", "model.cirx", "2");
    let bytes = Document::new(&entry, json!({"owner": 2}))
        .to_bytes(&limits)
        .unwrap();
    let key = CodecKey {
        kind: DocumentKind::from(entry.kind.as_str()),
        envelope_version: EnvelopeVersion::V1,
        schema_version: "2".into(),
    };
    for (writable, migratable) in [(false, true), (true, false), (true, true)] {
        let mut registry = CodecRegistry::new();
        registry
            .register(CodecRegistration {
                key: key.clone(),
                validate: |document, _| {
                    assert_eq!(document.id, "model");
                    Ok(())
                },
                writable,
                migratable,
            })
            .unwrap();
        let inspection = registry.inspect(&entry, &bytes, &limits).unwrap();
        assert!(matches!(inspection.document, DocumentRead::Envelope(_)));
        assert_eq!(inspection.capabilities.writable, writable);
        assert_eq!(inspection.capabilities.migratable, migratable);
        assert_eq!(
            registry.validate_migration(&entry, &bytes, &limits).is_ok(),
            writable && migratable
        );
        registry.validate_domain(&entry, &bytes, &limits).unwrap();
    }
    assert_eq!(
        decode_document(&entry, &bytes, &limits).unwrap(),
        DocumentRead::Opaque(&bytes)
    );
}

#[test]
fn json_scan_preserves_numeric_precision_and_checks_decoded_keys() {
    let limits = Limits::default();
    let entry = entry("number", "schematic", "number.cirsch", "1");
    let envelope = |payload| {
        format!(
            r#"{{"format":"cirnet.document","format_version":"1.0","id":"number","kind":"schematic","schema_version":"1","payload":{payload}}}"#
        )
    };
    let source = envelope(r#"{"long":900719925474099312345,"precise":0.123456789012345678901}"#);
    let DocumentRead::Envelope(document) =
        decode_document(&entry, source.as_bytes(), &limits).unwrap()
    else {
        panic!("expected supported envelope");
    };
    let canonical = String::from_utf8(document.to_bytes(&limits).unwrap()).unwrap();
    assert!(canonical.contains("900719925474099312345"));
    assert!(canonical.contains("0.123456789012345678901"));
    for number in ["-01", "1e+", "1..0", "1-2"] {
        assert!(matches!(
            decode_document(&entry, envelope(number).as_bytes(), &limits),
            Err(Error::Json(_))
        ));
    }
    for number in ["1e309", "1e-400"] {
        assert!(matches!(
            decode_document(&entry, envelope(number).as_bytes(), &limits),
            Err(Error::JsonPolicy {
                reason: "number outside the finite binary64 magnitude range",
                ..
            })
        ));
    }
    assert!(matches!(
        decode_document(
            &entry,
            envelope(r#"{"name":0,"n\u0061me":1}"#).as_bytes(),
            &limits
        ),
        Err(Error::JsonPolicy {
            reason: "duplicate object key",
            ..
        })
    ));
    assert!(matches!(
        decode_document(
            &entry,
            envelope(r#"{"\u0052efresh_token":""}"#).as_bytes(),
            &limits
        ),
        Err(Error::JsonPolicy {
            reason: "session credential or execution authority field",
            ..
        })
    ));
}

#[test]
fn graph_resolves_paths_and_keeps_hard_dependencies_apart_from_soft_cycles() {
    let limits = Limits::default();
    let mut manifest = Manifest::new("top", "Laboratory", vec!["electronics".into()]);
    manifest.documents = vec![
        entry("top", "schematic", "moved/main.cirsch", "1"),
        entry("board", "pcb", "board.cirpcb", "1"),
        entry("parts", "library", "parts.cirlib", "1"),
    ];
    manifest.links = vec![link(Relationship::Configuration, "top")];
    manifest.documents[0].links = vec![link(Relationship::Dependency, "board")];
    manifest.documents[1].links = vec![
        link(Relationship::Reference, "top"),
        link(Relationship::Library, "parts"),
    ];
    manifest.documents[2].links = vec![link(Relationship::Result, "top")];
    let graph = manifest.resource_graph(&limits).unwrap();
    let primary = ProjectPath::new("moved/main.cirsch").unwrap();
    assert_eq!(graph.get_by_path(&primary).unwrap().id(), "top");
    assert_eq!(
        graph
            .dependency_closure("top")
            .unwrap()
            .iter()
            .map(|resource| resource.id())
            .collect::<Vec<_>>(),
        vec!["board", "parts"]
    );
    assert_eq!(graph.outgoing(ResourceOwner::Project).len(), 1);
    assert_eq!(graph.outgoing(ResourceOwner::Document("top")).len(), 1);
    let impact = graph.impact("parts").unwrap();
    assert_eq!(impact.incoming.len(), 1);
    assert_eq!(
        impact.dependent_owners,
        vec![
            ResourceOwner::Document("board"),
            ResourceOwner::Document("top"),
            ResourceOwner::Project,
        ]
    );
    assert!(matches!(graph.impact("absent"), Err(Error::Missing(_))));

    manifest.documents[2]
        .links
        .push(link(Relationship::Dependency, "top"));
    assert!(matches!(
        manifest.resource_graph(&limits),
        Err(Error::Validation(_))
    ));
}
