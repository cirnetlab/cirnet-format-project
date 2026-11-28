# Application integration

| Consumer | Integration boundary |
| --- | --- |
| Interface and Studio | Populate Project Explorer with registry IDs, kinds, titles and links. Load a document only when requested. Ask its domain owner which editing capabilities the schema supports. |
| Remote | Authorize storage and transfer before format processing. Supply private, quota-controlled scratch space for a prepared import. Server leases, Lab control and physical approvals stay at their owning boundaries. |
| CLI | Expose inspect, verify and publish independently. Display inspection as metadata with unverified resources. Ask for an explicit, new destination for publication. |
| Desktop | Scope native access to the selected project, archive and scratch paths. Surface revision conflicts, recovery diagnostics and permissions to the caller. |
| WASM | Start with `--no-default-features` for byte APIs, or evaluate the package feature for seekable in-memory input. Qualify the selected target and implement browser storage in the consumer. Use lossless decoding for precision-sensitive payloads. |

Open a working snapshot with `Project::open`. Use registry IDs as stable
selection keys. Capture each affected or relied-on resource revision when
reading its content. Build a `ChangeSet` for native writes and use
`Project::apply` with an explicit codec registry for authored document
operations. Display successful reads and structural checks separately from
editor support. See [Transactions](../architecture/transactions.md).

For a `.cirpkg`, `package::inspect` reads metadata and
`package::verify_only` verifies a `Read + Seek` archive without destination
access. Use `package::export_from` with a `PackageSource` and a `Write + Seek`
sink for portable export. The native `package::verify` prepares an import in
scratch storage. Publish its verified result into a new, caller-controlled
directory. See [Package security](../architecture/package-security.md).

Contracts owns the shared CircuitGraph profiles. Pass the original bytes
to the owner validator and retain the owner-defined editor rules. Project
owns EngineeringProject lifecycle revisions, collaboration and access.
Core SHA-256 revisions only describe local file content. Lab-First
authorization remains with Remote and station protection with Hardware.
See the [owner routing table](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).
