# CIRNET Project Format Core

A standalone Rust library for working engineering projects and portable CIRNET LAB packages. The project format, document envelope and package profile are **1.0 candidates**. The Rust crate starts at **0.1.0**.

- [Format 1.0 specification and limits](spec/1.0/README.md)
- [Integration guide](docs/integration.md) · [Tiếng Việt](docs/vi/tich-hop.md)
- [Verification and benchmark results](docs/verification.md)
- [Licensing scope](LICENSING.md)

```rust
use cirnet_format_project::{Limits, Manifest, decode_manifest};

let limits = Limits::default();
let project = Manifest::new("project-01", "Bench supply", vec!["electronics".into()]);
let bytes = project.to_bytes(&limits)?;
assert_eq!(decode_manifest(&bytes, &limits)?.project_id, "project-01");
# Ok::<(), cirnet_format_project::Error>(())
```

The default `native` feature adds filesystem storage and ZIP. Use `--no-default-features` for byte-oriented manifest, document, migration, and integrity APIs. Generate the full API reference with `cargo doc --locked --all-features --no-deps`.

```sh
cargo test --locked --all-features
cargo run --locked --example working_project -- ./my-lab
cargo bench --locked --bench workloads
```

Examples create data only under their explicitly supplied working directory. The core performs no background sync, authorization, or execution.
