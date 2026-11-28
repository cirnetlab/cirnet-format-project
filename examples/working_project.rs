//! Create a small working project with an envelope and export a portable copy.
#[cfg(feature = "native")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cirnet_format_project::package::{Compression, export_file};
    use cirnet_format_project::storage::Project;
    use cirnet_format_project::transaction::{ChangeSet, ResourceWrite, WriteMode};
    use cirnet_format_project::{
        CodecKey, CodecRegistration, CodecRegistry, DocumentKind, EnvelopeVersion, Error,
    };
    use cirnet_format_project::{Document, DocumentEntry, Limits, Manifest, ProjectPath};

    let directory = std::env::args_os()
        .nth(1)
        .ok_or("supply a new working project directory")?;
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir(&directory)?;
    let limits = Limits::default();
    let mut manifest = Manifest::new("lab-01", "Engineering lab", vec!["electronics".into()]);
    let mut project = Project::create(directory.join("Lab.cirproj"), manifest.clone(), limits)?;
    let entry = DocumentEntry {
        id: "drawing".into(),
        kind: "schematic".into(),
        schema_version: "example.1".into(),
        path: ProjectPath::new("documents/Main.cirsch")?,
        title: Some("Main schematic".into()),
        links: vec![],
        extensions: Default::default(),
    };
    let bytes = Document::new(&entry, serde_json::json!({"example": true})).to_bytes(&limits)?;
    let path = entry.path.clone();
    manifest.documents.push(entry);
    let mut codecs = CodecRegistry::new();
    codecs.register(CodecRegistration {
        key: CodecKey {
            kind: DocumentKind::Schematic,
            envelope_version: EnvelopeVersion::V1,
            schema_version: "example.1".into(),
        },
        validate: |_, bytes| {
            let source: serde_json::Value = serde_json::from_slice(bytes)?;
            if source["payload"]["example"] == true {
                return Ok(());
            }
            Err(Error::Validation(
                "example owner rejected the schematic".into(),
            ))
        },
        writable: true,
        migratable: false,
    })?;
    let mut change = ChangeSet::new(project.revision().clone());
    change.write(ResourceWrite::new(
        "drawing",
        path,
        WriteMode::Create,
        bytes.as_slice(),
    ));
    change.set_manifest(manifest);
    project.apply(change, &codecs)?;
    export_file(directory.join("Lab.cirpkg"), &project, Compression::Deflate)?;
    Ok(())
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("Enable the native feature to run this example.");
}
