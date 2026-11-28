# CIRNET Project Format Core

[Tiếng Việt](../vi/index.md) · [Repository README](../../README.md)

Use the Rust library to inspect a working engineering project, save its
resources or exchange a portable package. The Rust crate, persisted format
profiles and domain schemas have independent version numbers.

## Use and integrate

- [Getting started](guides/getting-started.md) covers the native and portable APIs.
- [Integration](guides/integration.md) covers applications and tools.
- [Migration](guides/migration.md) covers 0.1.0 consumers and legacy CircuitGraph imports.

## Architecture and format

- [Architecture overview](architecture/overview.md) explains module and domain ownership.
- [Project model](architecture/project-model.md) describes files, identities and references.
- [Transactions and recovery](architecture/transactions.md) describes native writes.
- [Package security](architecture/package-security.md) describes archive and path boundaries.
- [Format 1.0 specification and schemas](../../spec/1.0/README.md) define the persisted formats.

- [CAD container proposal 0.1](architecture/cad-container-proposal-v0.1.md) defines the future representation boundary.

## Contribute and verify

- [Development](contributing/development.md) covers repository workflow.
- [Verification](contributing/verification.md) covers repository checks and evidence.
- [Writing documentation](contributing/writing-documentation.md) covers placement, translation and review.

## History and licensing

- [Changelog by major version](../changelogs/README.md) records changes in English.
- [0.1.0 verification and benchmark report](../history/v0.1.0-verification.md) preserves the original English Foundation evidence.
- [0.2.0 hardening qualification](../history/v0.2.0-hardening-qualification.md) records the earlier Windows checks and comparisons.
- [Licensing scope](../../LICENSING.md) describes the terms that apply.
