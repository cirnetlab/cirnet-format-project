# Project model

The hybrid format opens a project from a root-level `.cirproj` UTF-8 JSON
manifest. Editors can build an outline from the registry and read only the
resources needed for the current operation.

| Extension | Representation | Purpose |
| --- | --- | --- |
| `.cirproj` | UTF-8 JSON | Engineering project manifest |
| `.cirsch` | UTF-8 JSON envelope | Schematic |
| `.cirpcb` | UTF-8 JSON envelope | PCB |
| `.cir3d` | UTF-8 JSON envelope and model references | 3D assembly |
| `.cirlib` | UTF-8 JSON envelope and asset references | Engineering library |
| `.cirpkg` | ZIP32, JSON and native assets | Portable project package |

Preserve native formats for source, firmware, model, instrument and scientific
result files. The registry records their identity and locator. JSON envelopes
carry document metadata, structured domain payloads and extension data.
Canonical JSON keeps diffs readable. Retain exact source bytes for opaque or
legacy material that the Core only carries through.

## Resource identities and references

Project and resource IDs are stable across path changes. Document IDs, asset
IDs and external library IDs share one namespace. Resource graph queries index
IDs and paths, resolve forward and reverse references and compute dependency
closures. Build the graph with `Manifest::resource_graph` and use
`ResourceGraph::impact` to review incoming references and transitive owners
before removing or moving a resource. A relocation changes its locator while
references continue to use the same ID.

Document dependency and library edges must be acyclic. Ordinary references,
configuration and result links can form cycles. The graph deals in project
relationships. Payload references, pin connectivity and asset-specific
dependencies follow their owner schema. Native artifacts remain local registry
members. The external library coordinate is metadata until an authorized caller
resolves it.

## Document capabilities

Envelope inspection and structural decoding establish container-level facts.
Use `CodecRegistry` to determine domain capabilities by document kind, envelope
version and schema version. Register owner validation, writing and migration
capabilities explicitly through `CodecRegistration`. Unsupported documents can
be inspected and transferred
byte-for-byte when the operation allows preservation. Their registry identity
and unknown data remain available even when editing is unavailable.

See the [1.0 specification](../../../spec/1.0/README.md) for field, numeric,
relation and path constraints.
