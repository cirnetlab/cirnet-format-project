//! Verify the compatibility fixtures with a caller-selected Contracts CLI and source.
use cirnet_format_project::{Limits, ProjectPath, import_legacy_circuit};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn owner_check(cli: &Path, source: &Path, bytes: &[u8]) -> std::result::Result<(), String> {
    let mut child = Command::new(cli)
        .arg("validate")
        .arg(source)
        .args(["AnySupportedCircuitGraph", "structural"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let write = child
        .stdin
        .take()
        .expect("configured stdin")
        .write_all(bytes);
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    write.map_err(|error| error.to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = std::env::args_os().skip(1).collect::<Vec<_>>();
    let [cli, source] = paths.as_slice() else {
        return Err("usage: cargo run --example contracts_compat -- <trusted Contracts CLI> <Contracts source root>".into());
    };
    for fixture in [
        include_bytes!("../tests/fixtures/legacy-v1.0.json").as_slice(),
        include_bytes!("../tests/fixtures/legacy-v1.1.json").as_slice(),
        include_bytes!("../tests/fixtures/legacy-v1.2.json").as_slice(),
    ] {
        let path = ProjectPath::new("circuit/graph.json")?;
        let (manifest, original) = import_legacy_circuit(
            fixture,
            "migration",
            "Circuit migration",
            "legacy",
            path,
            &Limits::default(),
            |_, bytes| owner_check(Path::new(cli), Path::new(source), bytes),
        )?;
        assert_eq!(original, fixture);
        println!(
            "{} PASS, {} original bytes",
            manifest.documents[0].schema_version,
            original.len()
        );
    }
    Ok(())
}
