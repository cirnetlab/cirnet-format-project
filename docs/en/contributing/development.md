# Development

Run Cargo commands from the repository root. Use the declared `rust-version`
in `Cargo.toml` for the minimum compiler and a current stable toolchain for
the primary matrix. The lockfile pins the candidate dependency set. Keep
build outputs, temporary projects and benchmark evidence in allocated paths.

```sh
cargo fmt --all -- --check
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo doc --locked --all-features --no-deps
```

Keep one primary crate. Use the [source responsibility map](../architecture/overview.md)
when moving logic or publishing an API. Preserve regression tests and
compatibility fixtures. Add focused coverage for a changed trust boundary,
persistence operation or failure mode. Record a baseline and an equivalent
comparison when changing performance-sensitive behavior.

The [versioned specification](../../../spec/1.0/README.md) owns persisted
format semantics. Maintain its schemas and conformance fixtures together
with compatible runtime changes. Update matching paths under `docs/en/`
and `docs/vi/` in the same change and review the Vietnamese meaning before
refreshing its English source digest. Keep English release history in the
[shared changelog](../../changelogs/README.md).

Use [Verification](verification.md) for the full qualification and
benchmark workflow.
