# Application integration

| Consumer | Integration boundary |
| --- | --- |
| Project Manager | Use this crate for the portable manifest and registry, validated read sets, transactions, codecs and packages. Project Manager owns lifecycle and logical tree semantics. |
| Interface and Studio | Populate Project Explorer with registry IDs, kinds, titles and links. Load a document only when requested. Ask its domain owner which editing capabilities the schema supports. |
| Remote | Authorize storage and transfer before format processing. Supply private, quota-controlled scratch space for a prepared import. Server leases, Lab control and physical approvals stay at their owning boundaries. |
| CLI | Expose inspect, verify and publish independently. Display inspection as metadata with unverified resources. Ask for an explicit, new destination for publication. |
| Desktop | Scope native access to the selected project, archive and scratch paths. Surface revision conflicts, recovery diagnostics and permissions to the caller. |
| WASM | Start with `--no-default-features` for byte APIs, or evaluate the package feature for seekable in-memory input. Qualify the selected target and implement browser storage in the consumer. Use lossless decoding for precision-sensitive payloads. |

Open a working snapshot with `Project::open`. Use registry IDs as stable
selection keys. Capture the consulted documents and local resource revisions
with `Project::read_set`. Attach the tokens to a `ChangeSet` and use
`Project::apply_with_outcome` with the owner codec registry. Re-read on a
conflict, and inspect a published result before retrying a durability or
cleanup warning. Display structural checks separately from editor support.
See [Transactions](../architecture/transactions.md).

For a `.cirpkg`, `package::inspect` reads metadata and
`package::verify_only` verifies a `Read + Seek` archive without destination
access. Use `package::export_from` with a `PackageSource` and a `Write + Seek`
sink for portable export. The native `package::verify` prepares an import in
scratch storage. Publish its verified result into a new, caller-controlled
directory with `publish_retryable`. See [Package security](../architecture/package-security.md).

The [standalone Rust consumer](../../../examples/qualified_consumer.rs)
exercises these APIs using test-only domain schemas. It qualifies the local
library workflow. Product integration, browser persistence and execution
authorization require separate evidence.

Contracts owns the shared CircuitGraph profiles. Pass the original bytes
to the owner validator and retain the owner-defined editor rules.
[Project Manager](https://github.com/cirnetlab/cirnet-project-manager) owns
EngineeringProject lifecycle revisions and artifact references.
Core SHA-256 revisions describe local file content. Server revisions and
Lab-First authorization remain with the current service owner, Remote.
Hardware retains station protection.
See the [owner routing table](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).
