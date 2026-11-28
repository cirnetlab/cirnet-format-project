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
| Revision checks, staged changes and recovery | `src/transaction/` |
| Working project and filesystem operations | `src/storage/native/` |
| Consumer-facing entry points, errors and input limits | `src/lib.rs`, `src/error.rs`, `src/limits.rs` |

Portable algorithms consume bytes or standard I/O traits. Native storage
supplies filesystem operations and coordinates project writes. The public Rust
API is documented from the crate source. The versioned
[format specification](../../../spec/1.0/README.md) defines persisted semantics.

Shared wire schemas and generated SDKs belong to Contracts. EngineeringProject
lifecycle, ACLs and service revision IDs belong to Project. A local SHA-256
revision is an optimistic content token. Interface can project working data
into an editor. In Lab-First flows, Remote authorizes operations and manages
leases, and Hardware owns station enforcement. See the authoritative
[CIRNETLAB ownership routing](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).
