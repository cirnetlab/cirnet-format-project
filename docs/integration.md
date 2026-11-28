# Integrating the Project Format Core

- **Interface** opens a manifest snapshot and projects document IDs, kinds, titles and relationships into Project Explorer. Lazy reads return format capabilities separately from editor support. Pass exact CircuitGraph bytes to the owner codec and honor the owner's editor validation. No browser binding is shipped.
- **Remote** continues to authorize server storage, leases and execution. It may validate an already-authorized transfer using `package::verify` in a private, quota-controlled scratch area. Publish only to a new, caller-controlled local directory. Archive integrity gives no authority to execute anything.
- **CLI** can provide separate inspect, verify and extract commands over the Rust library. Show an inspect result as metadata with unchecked payloads. Require an explicit output location for publishing verified bytes.
- **WASM** can target `--no-default-features` and supply byte/storage adapters in a future binding. Keep numerical parsing lossless at the boundary. Cross-runtime version and schema compatibility need evidence in the integrating repo.
- **Project** owns EngineeringProject ownership, revisions and collaboration. A SHA-256 content revision is an optimistic local save token, independent of a lifecycle revision.

## Staged filesystem writes

`Project::open` reads and indexes only the manifest. `read_document` reads a bounded authored file and supplies its digest token. `save_document` checks the manifest token, the expected document digest and the caller's domain validation. Opaque document versions can be carried through the package byte for byte. `Project::commit` prepares files with new, unoccupied paths, syncs their bytes, installs them without overwriting, and atomically replaces the manifest last. A process interruption before replacement leaves the old manifest readable. Files or empty directories from the interrupted change may be unreferenced. Inspect those locally before explicit cleanup. Unreferenced content is excluded during package export.

The advisory lock coordinates writers using this library, including export. Digest checks catch stale snapshots and changes made before the final check. Place the working root and package scratch space in trusted local directories, with exclusive writer access where possible. Existing symlinks and Windows reparse points inside a project are refused. The filesystem adapter assumes a concurrent process cannot replace path components in caller-controlled directories during an operation. Imported filenames and bytes never get that authority. Unix directory sync preserves rename ordering where supported. Windows syncs file content and uses same-directory replacement. Durability across OS crash or power loss depends on filesystem and device behavior. Shared/network drives and adversarial concurrent local directory replacement have no qualification.

A verified import publishes its manifest last in a directory created exclusively for that import. An interruption before manifest publication may leave an incomplete directory with no manifest. A Durability error identifies a file that was installed before its directory sync failed. Inspect that path and reopen the project before retrying. Choose a new destination for retry or independently review and remove the incomplete directory.

## Contracts-owned compatibility

The [Contracts CLI adapter example](../examples/contracts_compat.rs) accepts explicit paths to a trusted owner executable and its compatible source/resource root. It calls the owner's `AnySupportedCircuitGraph` structural profile for the supplied 1.0, 1.1 and 1.2 fixtures and keeps the original bytes. Validation in an editor or execution context remains owned by those components.

Core contains no duplicated CircuitGraph schema and remains independent of the application and network. Keep incompatible format revisions behind explicit version negotiation.
