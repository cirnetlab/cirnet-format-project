# Migration

## Move a 0.1.0 consumer to 0.2.0

Existing supported project manifests, document envelopes and ZIP packages retain
their 1.0 profiles. Crate 0.2.0 versions these profiles independently from one
another and from domain schemas and shared Contracts. Updating the Rust
dependency alone requires no rewrite of a supported project.

1. Select the required Cargo features and rebuild the consumer.
2. Move resource changes from `Project::commit` to `Project::apply`.
   Supply a `ChangeSet` with the expected manifest revision and the digests
   of affected resources and declared read dependencies. Register the domain
   codecs that validate authored content. Restrict the retained
   `Project::commit` entry point to metadata-only changes.
3. Keep ordinary single-document writes on `Project::save_document` when
   its revision and validator inputs cover the operation. Use a change set
   when the operation has hard dependencies or relies on other local files.
4. On a revision conflict, reload the affected state and reapply the user
   edit. Resolve any pending local recovery before changing implementations
   or moving the working directory.

Use the intent appropriate to the file operation. An opaque unsupported
document can relocate with its original bytes. Authoring a different envelope
or schema requires a supported, explicit migration path. For a native
migration, register migration-capable codecs for both source and destination.
Use the format version detector before selecting a migration. An owner
transition must produce its declared target, and the target must pass the
applicable checks.
Retain the source bytes and verify the result before replacing project state.

## Import legacy CircuitGraph

The [Contracts adapter example](../../../examples/contracts_compat.rs) imports
CircuitGraph 1.0, 1.1 and 1.2. Supply the path to a trusted Contracts CLI and
its compatible source root.

```sh
cargo run --locked --example contracts_compat -- <trusted-cli> <contracts-source-root>
```

Core passes exact input bytes to the Contracts `AnySupportedCircuitGraph`
structural profile. The original bytes stay intact as a registered graph
file. Keep the manifest resource ID distinct from the domain `circuit_id`.
The editor or execution owner must perform any context-specific validation.
For unsupported versions, retain the original input and select an owner
transition when available.
