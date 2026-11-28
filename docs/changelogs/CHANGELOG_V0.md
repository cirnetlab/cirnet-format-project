# CIRNET Project Format version 0 changelog

[Changelog index](README.md)

## Unreleased

### Changed

- Standardize shared English major-version changelog navigation and add
  paired English and Vietnamese documentation maintenance guides.
- Decode current manifests directly after strict JSON preflight. Retain
  version diagnostics and the full resource graph validation.

- Clarify Project Manager ownership and document the proposed CAD
  representation boundary while preserving Project Format 1.0.

### Added

- Add bounded validated read sets, structured transaction outcomes and an
  explicit path for changing registered resource metadata.
- Register exact owner migration edges and qualify a standalone Rust consumer.
- Compare Draft 2020-12 schema vectors with runtime behavior and exercise
  bounded parser, archive and recovery mutations.

### Fixed

- Clean up operation-owned package files after pre-publication cancellation
  when ownership is verifiable. Report retained destinations and allow
  retry from a verified stage after clean rejection.

### Security

- Require migration authorization for the exact source and destination
  codecs, with owner checks on the original and proposed document bytes.

## v0.2.0 — Development candidate

Rust crate 0.2.0. The Project Manifest, Document Envelope and Package Profile
remain independent 1.0 candidates. Existing supported files require no format
conversion for this crate update.

### Added

- Add explicit changesets with revision checks and a recovery journal for
  interrupted manifest-changing transactions.
- Add typed document codec registration and indexed resource lookup by ID and
  path, with forward, reverse and transitive hard-dependency queries.
- Expose portable ZIP inspection and verification with progress and cooperative
  cancellation. Keep verified staging and publication in the native adapter.

### Changed

- Organize the crate around manifest, document, resource graph, serialization,
  integrity, migration, package processing, transactions and native storage.
- Give the 1.0 manifest, envelope and package profiles independent version
  handling. Preserve byte-exact Contracts-validated CircuitGraph 1.0–1.2 imports.
- Route resource changes through `Project::apply`. The compatibility
  `Project::commit` entry point accepts metadata-only changes. Resource IDs
  remain stable during relocation.
- Pair development and integration guidance in English and Vietnamese. Retain
  the original 0.1.0 verification and benchmark evidence.

### Fixed

- Reject stale resource and dependency revisions, including writes based on
  a document saved independently without a manifest revision change.

### Security

- Require domain-owner validation for authored document changes and protect
  opaque documents by requiring byte-identical transfers on relocation.

**Sources**

- [Version manifest at the candidate revision](https://github.com/cirnetlab/cirnet-format-project/blob/1104211ff80fc7a989e0be952983c8f32a2860cf/Cargo.toml)
- [0.2.0 candidate source revision](https://github.com/cirnetlab/cirnet-format-project/commit/1104211ff80fc7a989e0be952983c8f32a2860cf)
- [Migration guide](../en/guides/migration.md)
- [Candidate verification instructions](../en/contributing/verification.md#current-candidate-checks)

## v0.1.0 — Foundation development milestone

### Added

- Establish the Rust Project Format 1.0 candidate with versioned manifests,
  document envelopes and stable references.
- Import original CircuitGraph bytes after Contracts validation. Add staged
  filesystem saves and SHA-256-verified ZIP packaging.
- Add format schemas, compatibility fixtures, CI and release-mode benchmarks.

**Sources**

- [Version manifest at the Foundation revision](https://github.com/cirnetlab/cirnet-format-project/blob/cc3a257e03560943b531341d1520bf7f0892eb2c/Cargo.toml)
- [0.1.0 Foundation source revision](https://github.com/cirnetlab/cirnet-format-project/commit/cc3a257e03560943b531341d1520bf7f0892eb2c)
- [Original 0.1.0 verification and benchmark report](../history/v0.1.0-verification.md)
