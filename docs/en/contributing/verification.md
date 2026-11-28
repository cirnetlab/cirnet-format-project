# Verification

Run the checks from the repository root. Record the revision or source
artifact, target and toolchain, exact commands, logs and SHA-256 for any
shared artifacts. Use `PASS`, `FAIL`, `NOT_RUN` and `BLOCKED` for observed
outcomes or qualification gaps.

## Current candidate checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features package --all-targets -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features package
cargo build --locked --release --all-features
cargo build --locked --release --no-default-features
cargo build --locked --release --no-default-features --features package
cargo doc --locked --all-features --no-deps
```

- Run the native matrix on Windows, Linux and macOS using the
  [verification workflow](../../../.github/workflows/verify.yml). On Windows,
  run `cargo test --locked --test native symlinked_native_resources_are_rejected -- --ignored`
  where the fixture can create a junction.
- Repeat required build and test lanes with the compiler in `rust-version`
  from `Cargo.toml`. A declared compiler version requires its own evidence.
  With the WASM target installed, run
  `cargo check --locked --no-default-features --target wasm32-unknown-unknown`
  and separately qualify the portable package feature when supported.
- Validate generated manifests, envelopes and inventories with a Draft
  2020-12 checker and compare with strict runtime behavior. Test the
  [Contracts compatibility example](../../../examples/contracts_compat.rs)
  with a trusted owner CLI and source revision. Record both identities.
- Exercise stale cross-operation revisions, rejected validation, recovery
  phases, path conflicts, opaque bytes, malformed JSON and archive bounds.
  Record property or fuzz tool, corpus, target, duration and findings
  when running parser or archive fuzzing.

## Documentation checks

From this repository root, use `check-docs --product --root .` from an approved
CIRNET Rules checkout. Pass `--new-file <path>` for each added Markdown page.
Run `git diff --check` and confirm the diff matches the documentation scope.

Review relative links and heading anchors, matching English and Vietnamese
paths, translation meaning and source hashes, and changelog provenance.
See [writing documentation](writing-documentation.md) for conventions.
Run a targeted build check when an edited path is used by compilation,
packaging, tests or CI.

## Reproduce performance evidence

Use `cargo bench --locked --bench workloads` and record workload sizes, sample
count, build mode, filesystem and cache conditions. Compare like operations
with the same input, profile, machine and output storage. Report latency
distributions, throughput for byte workloads and peak memory measurement
method. Inspect, verify-only, verify-and-stage and full publication measure
different work. Record any sync or copy costs independently where measured.

## Historical evidence

The original [0.1.0 verification and benchmark report](../../history/v0.1.0-verification.md)
records local Windows Foundation results, baseline provenance and artifact
checksums. The
[baseline CI run](https://github.com/cirnetlab/cirnet-format-project/actions/runs/46088261341)
applies to [commit `cc3a257`](https://github.com/cirnetlab/cirnet-format-project/commit/cc3a257e03560943b531341d1520bf7f0892eb2c).
Use fresh runs and measurements to qualify crate 0.2.0.
