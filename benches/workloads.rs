use cirnet_format_project::package::{Compression, export_file, verify};
use cirnet_format_project::storage::{Project, StagedFile};
use cirnet_format_project::{
    AssetEntry, Document, DocumentEntry, Limits, Manifest, ProjectPath, hash_reader,
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
    let mut stages: Vec<StagedFile<Box<dyn std::io::Read>>> = Vec::new();
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
        stages.push(StagedFile::new(
            entry.path.clone(),
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
    stages.push(StagedFile::new(asset.path.clone(), Box::new(file)));
    next.assets.push(asset);
    project.commit(next, stages).unwrap();
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

fn main() {
    println!(
        "CIRNET release benchmark: {TRIALS} samples after one warm-up, {}, {} threads available",
        std::env::consts::OS,
        std::thread::available_parallelism().map_or(1, usize::from)
    );
    let area = tempfile::tempdir().unwrap();
    for (label, documents, mib) in [("small", 20, 1), ("medium", 200, 16), ("large", 1_000, 96)] {
        workload(area.path(), label, documents, mib);
    }
    if std::env::var_os("CIRNET_BENCH_MEASURE_MEMORY").is_some() {
        // Allow the optional external sampler to observe the process after the final sample.
        std::thread::sleep(Duration::from_millis(200));
    }
}
