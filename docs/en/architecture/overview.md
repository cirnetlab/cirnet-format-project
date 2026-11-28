# Architecture

CIRNET Project Format Core owns portable project serialization. It supports
Electronics, PCB Design, 3D Assembly, Embedded Systems, FPGA, Robotics, IoT,
Scientific Computing, and Virtual and Physical Lab projects through manifests,
document envelopes, registered relationships and opaque native assets. Domain
owners define the meaning of schematic, PCB, CAD, capture, library and other
payloads. Those domain engines can use the format through caller-supplied codecs.

| Responsibility | Source boundary |
| --- | --- |
| Project metadata, registry invariants and manifest codec | `src/manifest/` |
| Document envelopes and registered codec capabilities | `src/document/` |
| Resource identity, portable paths, references and dependency queries | `src/resource/` |
| Bounded deterministic JSON | `src/serialization/` |
| Version detection, explicit transitions and legacy CircuitGraph import | `src/migration/` |
| Digests and package inventory | `src/integrity/` |
| ZIP profile, preflight, reading, writing and verification | `src/package/` |
| Planning, read sets, revision checks, publication and recovery | `src/transaction/` |
| Working project and filesystem operations | `src/storage/native/` |
| Consumer-facing entry points, errors and input limits | `src/lib.rs`, `src/error.rs`, `src/limits.rs` |

Portable algorithms consume bytes or standard I/O traits. Native storage
supplies filesystem operations and coordinates project writes. The public Rust
API is documented from the crate source. The versioned
[format specification](../../../spec/1.0/README.md) defines persisted semantics.

[Project Manager](https://github.com/cirnetlab/cirnet-project-manager) owns
EngineeringProject and document lifecycle, logical organization, lifecycle
revisions and artifact references. It consumes Format Core through public Rust
APIs. A Format SHA-256 revision is an optimistic byte-content token. Shared wire
schemas and generated SDKs belong to Contracts. Interface owns editor
presentation. Server persistence and authorization remain with Remote in the
current Lab implementation. Hardware owns station enforcement. See the authoritative
[CIRNETLAB ownership routing](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).

The [CAD container architecture proposal](cad-container-proposal-v0.1.md)
separates .cir3d assembly placement from editable CAD model data and describes
the planned representation boundary.
