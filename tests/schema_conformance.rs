use cirnet_format_project::{
    DocumentEntry, DocumentRead, Limits, ProjectPath, decode_document, decode_manifest,
};
use serde::Deserialize;
use serde_json::Value;

#[cfg(feature = "package")]
#[path = "support/archive.rs"]
mod archive;

#[derive(Deserialize)]
struct Corpus {
    #[cfg(feature = "package")]
    archive_manifest: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    schema: Kind,
    runtime: Disposition,
    input: Input,
    legacy_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Manifest,
    Document,
    Integrity,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Accept,
    Opaque,
    Reject,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum Input {
    Json(Value),
    Raw(String),
    Fixture(String),
}

impl Input {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Json(value) => serde_json::to_vec(value).unwrap(),
            Self::Raw(text) => text.as_bytes().to_vec(),
            Self::Fixture(name) => match name.as_str() {
                "legacy-v1.0.json" => include_bytes!("fixtures/legacy-v1.0.json").to_vec(),
                "legacy-v1.1.json" => include_bytes!("fixtures/legacy-v1.1.json").to_vec(),
                "legacy-v1.2.json" => include_bytes!("fixtures/legacy-v1.2.json").to_vec(),
                _ => panic!("unknown schema conformance fixture: {name}"),
            },
        }
    }
}

fn accept_if(valid: bool) -> Disposition {
    if valid {
        Disposition::Accept
    } else {
        Disposition::Reject
    }
}

#[test]
fn shared_draft_2020_12_cases_cover_runtime_constraints() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/schema-conformance.json")).unwrap();
    let limits = Limits::default();
    let mut checked = 0;
    for case in corpus.cases {
        let bytes = case.input.bytes();
        let actual = match case.schema {
            Kind::Manifest => accept_if(decode_manifest(&bytes, &limits).is_ok()),
            Kind::Document => {
                let (kind, version, path) = case
                    .legacy_version
                    .as_deref()
                    .map_or(("schematic", "test.1", "drawing.cirsch"), |version| {
                        ("circuit-graph", version, "drawing.json")
                    });
                let entry = DocumentEntry {
                    id: "drawing".into(),
                    kind: kind.into(),
                    schema_version: version.into(),
                    path: ProjectPath::new(path).unwrap(),
                    title: None,
                    links: vec![],
                    extensions: Default::default(),
                };
                match decode_document(&entry, &bytes, &limits) {
                    Ok(DocumentRead::Envelope(document)) => {
                        assert_eq!(
                            decode_document(&entry, &document.to_bytes(&limits).unwrap(), &limits)
                                .unwrap(),
                            DocumentRead::Envelope(document)
                        );
                        Disposition::Accept
                    }
                    Ok(DocumentRead::LegacyCircuit(original)) => {
                        assert_eq!(original, bytes);
                        Disposition::Accept
                    }
                    Ok(DocumentRead::Opaque(original)) => {
                        assert_eq!(original, bytes);
                        Disposition::Opaque
                    }
                    Err(_) => Disposition::Reject,
                }
            }
            #[cfg(feature = "package")]
            Kind::Integrity => accept_if(
                cirnet_format_project::package::verify_only(
                    archive::with_inventory(corpus.archive_manifest.as_bytes(), &bytes),
                    limits,
                )
                .is_ok(),
            ),
            #[cfg(not(feature = "package"))]
            Kind::Integrity => continue,
        };
        assert_eq!(actual, case.runtime, "{}", case.name);
        checked += 1;
    }
    eprintln!("{checked} shared conformance cases checked against the runtime");
}
