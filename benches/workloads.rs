use cirnet_format_project::package::{Compression, export_file, inspect, verify, verify_only};
use cirnet_format_project::storage::Project;
use cirnet_format_project::transaction::{ChangeSet, ResourceWrite, WriteMode};
use cirnet_format_project::{
    AssetEntry, Document, DocumentEntry, Limits, Manifest, ProjectPath, hash_reader,
};
use cirnet_format_project::{
    CodecKey, CodecRegistration, CodecRegistry, DocumentKind, EnvelopeVersion, Link, Relationship,
};
use std::{
    fs::{self, File},
    hint::black_box,
    io::{Cursor, Write},
    path::Path,
    time::{Duration, Instant},
};

const TRIALS: usize = 7;
const LOOKUPS: usize = 100_000;

fn p(path: impl Into<String>) -> ProjectPath {
    ProjectPath::new(path).unwrap()
}

fn measure<T>(samples: &mut Vec<f64>, f: impl FnOnce() -> T) -> T {
    let time = Instant::now();
    let result = black_box(f());
    samples.push(time.elapsed().as_secs_f64() * 1000.0);
    result
}

fn summary(label: &str, mut times: Vec<f64>) {
    times.sort_by(f64::total_cmp);
    let p95 = ((times.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    println!(
        "{label:<21} {:>10.3} {:>10.3} {:>10.3}",
        times[0],
        times[times.len() / 2],
        times[p95]
    );
}
fn rate(label: &str, bytes: u64, samples: &[f64]) {
    let mib = bytes as f64 / 1048576.0;
    let ms = samples.iter().copied().sum::<f64>() / samples.len() as f64;
    println!(
        "{label}: {:.1} MiB/s (input bytes / mean elapsed)",
        mib / (ms / 1000.0)
    );
}

fn corpus(folder: &Path, documents: usize, asset_mib: u64) -> Project {
    fs::create_dir(folder).unwrap();
    let limits = Limits::default();
    let initial = Manifest::new("benchmark", "Benchmark", vec!["electronics".into()]);
    let mut project = Project::create(folder.join("lab.cirproj"), initial.clone(), limits).unwrap();
    let mut next = initial;
    let mut change: ChangeSet<Box<dyn std::io::Read>> = ChangeSet::new(project.revision().clone());
    for index in 0..documents {
        let entry = DocumentEntry {
            id: format!("doc-{index}"),
            path: p(format!("docs/doc-{index}.cirsch")),
            kind: "schematic".into(),
            schema_version: "test.1".into(),
            title: Some(format!("Drawing {index}")),
            links: vec![],
            extensions: Default::default(),
        };
        let data = Document::new(
            &entry,
            serde_json::json!({"synthetic":"x".repeat(2048),"revision":0}),
        )
        .to_bytes(&limits)
        .unwrap();
        change.write(ResourceWrite::new(
            &entry.id,
            entry.path.clone(),
            WriteMode::Create,
            Box::new(Cursor::new(data)),
        ));
        next.documents.push(entry);
    }
    let asset = AssetEntry {
        id: "model".into(),
        path: p("assets/bench.bin"),
        media_type: "application/octet-stream".into(),
        extensions: Default::default(),
    };
    let mut file = tempfile::tempfile().unwrap();
    let mut state = 0x42c0_ffee_u32;
    let mut block = vec![0; 1024 * 1024];
    for byte in &mut block {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = state as u8;
    }
    for _ in 0..asset_mib {
        file.write_all(&block).unwrap();
    }
    std::io::Seek::rewind(&mut file).unwrap();
    change.write(ResourceWrite::new(
        &asset.id,
        asset.path.clone(),
        WriteMode::Create,
        Box::new(file),
    ));
    next.assets.push(asset);
    let registry = benchmark_registry();
    change.set_manifest(next);
    project.apply(change, &registry).unwrap();
    project
}

fn workload(area: &Path, label: &str, documents: usize, asset_mib: u64) {
    let folder = area.join(label);
    let project = corpus(&folder, documents, asset_mib);
    let manifest = folder.join("lab.cirproj");
    let limits = *project.limits();
    let manifest_size = fs::metadata(&manifest).unwrap().len();
    let index = project.manifest().index(&limits).unwrap();
    let keys: Vec<_> = project
        .manifest()
        .documents
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let mut open = vec![];
    let mut read = vec![];
    let mut write = vec![];
    let mut lookup = vec![];
    let mut package = vec![];
    let mut integrity = vec![];
    let mut hashing = vec![];
    let mut archive_bytes = 0;
    for trial in 0..=TRIALS {
        let mut discard = Vec::new();
        let measured = trial != 0;
        measure(if measured { &mut open } else { &mut discard }, || {
            Project::open(&manifest, limits).unwrap()
        });
        let last = keys.last().unwrap();
        let snapshot = measure(if measured { &mut read } else { &mut discard }, || {
            project.read_document(last).unwrap()
        });
        let entry = project.manifest().documents.last().unwrap();
        let bytes = Document::new(
            entry,
            serde_json::json!({"synthetic":"x".repeat(2048),"revision":trial+1}),
        )
        .to_bytes(&limits)
        .unwrap();
        measure(if measured { &mut write } else { &mut discard }, || {
            project
                .save_document(last, &snapshot.revision, &bytes, |_, _| Ok(()))
                .unwrap()
        });
        measure(if measured { &mut lookup } else { &mut discard }, || {
            for index_no in 0..LOOKUPS {
                black_box(index.get(black_box(keys[index_no % keys.len()])));
            }
        });
        let bundle = area.join(format!("{label}-{trial}.cirpkg"));
        measure(if measured { &mut package } else { &mut discard }, || {
            export_file(&bundle, &project, Compression::Deflate).unwrap()
        });
        archive_bytes = fs::metadata(&bundle).unwrap().len();
        let verified = measure(
            if measured {
                &mut integrity
            } else {
                &mut discard
            },
            || verify(File::open(&bundle).unwrap(), area, limits).unwrap(),
        );
        drop(verified);
        fs::remove_file(bundle).unwrap();
        let file = project.open_asset("model").unwrap();
        measure(if measured { &mut hashing } else { &mut discard }, || {
            hash_reader(file, limits.max_asset_bytes).unwrap()
        });
    }
    println!(
        "\n{label}: {documents} docs, {asset_mib} MiB asset, {manifest_size} B manifest, {archive_bytes} B ZIP"
    );
    println!(
        "{:<21} {:>10} {:>10} {:>10}",
        "milliseconds", "min", "median", "p95"
    );
    rate("ZIP export", archive_bytes, &package);
    rate("ZIP verify and stage", archive_bytes, &integrity);
    rate("SHA-256", asset_mib * 1048576, &hashing);
    for (label, samples) in [
        ("Manifest open", open),
        ("Document read", read),
        ("Document save", write),
        ("Indexed lookups x100k", lookup),
        ("ZIP export", package),
        ("ZIP verify and stage", integrity),
        ("SHA-256", hashing),
    ] {
        summary(label, samples);
    }
}

fn benchmark_registry() -> CodecRegistry {
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

fn advanced(area: &Path) {
    let folder = area.join("advanced");
    fs::create_dir(&folder).unwrap();
    let limits = Limits::default();
    let mut manifest = Manifest::new(
        "advanced",
        "Dense dependency benchmark",
        vec!["electronics".into()],
    );
    let mut project =
        Project::create(folder.join("lab.cirproj"), manifest.clone(), limits).unwrap();
    let registry = benchmark_registry();
    let mut setup: ChangeSet<Box<dyn std::io::Read>> = ChangeSet::new(project.revision().clone());
    let large = "x".repeat(1024 * 1024);
    for index in 0usize..256 {
        let entry = DocumentEntry {
            id: format!("doc-{index}"),
            kind: "schematic".into(),
            schema_version: "test.1".into(),
            path: p(format!("docs/{index}.cirsch")),
            title: None,
            links: (index.saturating_sub(8)..index)
                .map(|parent| Link {
                    relationship: Relationship::Dependency,
                    target: format!("doc-{parent}"),
                })
                .collect(),
            extensions: Default::default(),
        };
        let payload = if index < 2 {
            serde_json::json!({"blob":large})
        } else {
            serde_json::json!({"index":index})
        };
        let bytes = Document::new(&entry, payload).to_bytes(&limits).unwrap();
        setup.write(ResourceWrite::new(
            &entry.id,
            entry.path.clone(),
            WriteMode::Create,
            Box::new(Cursor::new(bytes)),
        ));
        manifest.documents.push(entry);
    }
    for index in 0..2 {
        let asset = AssetEntry {
            id: format!("asset-{index}"),
            path: p(format!("assets/{index}.bin")),
            media_type: "application/octet-stream".into(),
            extensions: Default::default(),
        };
        let input = tempfile::tempfile().unwrap();
        input.set_len(8 * 1024 * 1024).unwrap();
        setup.write(ResourceWrite::new(
            &asset.id,
            asset.path.clone(),
            WriteMode::Create,
            Box::new(input),
        ));
        manifest.assets.push(asset);
    }
    setup.set_manifest(manifest);
    project.apply(setup, &registry).unwrap();
    let archive = area.join("advanced.cirpkg");
    export_file(&archive, &project, Compression::Deflate).unwrap();
    let archive_size = fs::metadata(&archive).unwrap().len();

    let mut graph_time = vec![];
    let mut large_json = vec![];
    let mut transaction = vec![];
    let mut inspect_time = vec![];
    let mut verify_time = vec![];
    let mut stage_time = vec![];
    let mut publish_time = vec![];
    for trial in 0..=TRIALS {
        let mut discarded = vec![];
        let measured = trial != 0;
        measure(
            if measured {
                &mut graph_time
            } else {
                &mut discarded
            },
            || {
                let graph = project.manifest().resource_graph(&limits).unwrap();
                assert_eq!(graph.dependency_closure("doc-255").unwrap().len(), 255);
            },
        );
        measure(
            if measured {
                &mut large_json
            } else {
                &mut discarded
            },
            || project.read_document("doc-1").unwrap(),
        );
        let mut next = project.manifest().clone();
        let mut changes = ChangeSet::new(project.revision().clone());
        for doc in &mut next.documents[..2] {
            changes.expect(&doc.id, project.resource_revision(&doc.id).unwrap());
            doc.path = p(format!("revision-{trial}/{}.cirsch", doc.id));
            let bytes = Document::new(doc, serde_json::json!({"blob":large,"trial":trial}))
                .to_bytes(&limits)
                .unwrap();
            changes.write(ResourceWrite::new(
                &doc.id,
                doc.path.clone(),
                WriteMode::Replace,
                Cursor::new(bytes),
            ));
        }
        changes.set_manifest(next);
        measure(
            if measured {
                &mut transaction
            } else {
                &mut discarded
            },
            || project.apply(changes, &registry).unwrap(),
        );
        measure(
            if measured {
                &mut inspect_time
            } else {
                &mut discarded
            },
            || inspect(File::open(&archive).unwrap(), limits).unwrap(),
        );
        measure(
            if measured {
                &mut verify_time
            } else {
                &mut discarded
            },
            || verify_only(File::open(&archive).unwrap(), limits).unwrap(),
        );
        let verified = measure(
            if measured {
                &mut stage_time
            } else {
                &mut discarded
            },
            || verify(File::open(&archive).unwrap(), area, limits).unwrap(),
        );
        measure(
            if measured {
                &mut publish_time
            } else {
                &mut discarded
            },
            || {
                verified
                    .publish(area.join(format!("published-{trial}")))
                    .unwrap()
            },
        );
    }
    println!(
        "\nadvanced: 256 documents, 2012 hard edges, two 1 MiB JSON payloads, two 8 MiB assets, {archive_size} B ZIP"
    );
    println!(
        "{:<21} {:>10} {:>10} {:>10}",
        "milliseconds", "min", "median", "p95"
    );
    rate("ZIP verify-only", archive_size, &verify_time);
    for (label, samples) in [
        ("Dense graph + closure", graph_time),
        ("Large document read", large_json),
        ("Two-document commit", transaction),
        ("ZIP inspect", inspect_time),
        ("ZIP verify-only", verify_time),
        ("ZIP verify and stage", stage_time),
        ("Verified publication", publish_time),
    ] {
        summary(label, samples);
    }
}

fn main() {
    println!(
        "CIRNET release benchmark: {TRIALS} samples after one warm-up, {}, {} threads available",
        std::env::consts::OS,
        std::thread::available_parallelism().map_or(1, usize::from)
    );
    let area = tempfile::tempdir().unwrap();
    if std::env::args().any(|arg| arg == "--advanced") {
        advanced(area.path());
    } else {
        for (label, documents, mib) in [("small", 20, 1), ("medium", 200, 16), ("large", 1_000, 96)]
        {
            workload(area.path(), label, documents, mib);
        }
    }
    if std::env::var_os("CIRNET_BENCH_MEASURE_MEMORY").is_some() {
        // Allow the optional external sampler to observe the process after the final sample.
        std::thread::sleep(Duration::from_millis(200));
    }
}
