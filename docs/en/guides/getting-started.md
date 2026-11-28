# Getting started

The crate provides portable manifest, document, migration, resource and
integrity APIs with default features disabled. Enable `package` for portable
ZIP processing, or use the default `native` feature for filesystem-backed
working projects and package import and export.

## Read and write manifest bytes

```rust
use cirnet_format_project::{Limits, Manifest, decode_manifest};

let limits = Limits::default();
let manifest = Manifest::new("project-01", "Bench supply", vec!["electronics".into()]);
let bytes = manifest.to_bytes(&limits)?;
let loaded = decode_manifest(&bytes, &limits)?;
assert_eq!(loaded.project_id, "project-01");
# Ok::<(), cirnet_format_project::Error>(())
```

## Create a working project

From the repository root, choose a new output directory and run the
[working project example](../../../examples/working_project.rs).

```sh
cargo run --locked --example working_project -- ./my-lab
```

The example creates a native project, writes an authored envelope through a
registered validator for its example schema, and exports a `.cirpkg`. Use
`Project::open` with a chosen `.cirproj` path for an existing project. Opening
loads the manifest. `read_document` loads the requested file and returns both
bytes and the revision needed for a subsequent write.

Generate the crate reference with `cargo doc --locked --all-features --no-deps`.
See [Integration](integration.md) for application and service boundaries.
