# CIRNET Project Format Core repository instructions

This repository owns portable project serialization, the project manifest and
document envelope, the resource registry and graph, native working-directory
transactions, the ZIP package profile, and format compatibility fixtures.
Read the common agent rules, architecture policy and ownership routing from
an approved Rules checkout or verified artifact.
[Common rules](https://github.com/cirnetlab/cirnet-rules/blob/master/AGENTS.md) ·
[Architecture policy](https://github.com/cirnetlab/cirnet-rules/blob/master/en/policies/architecture.md) ·
[Ownership routing](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md)

Retain domain semantics in their owner repositories. Contracts owns shared CircuitGraph schemas,
Project owns its lifecycle revisions and authorization, and Remote and
Hardware own their Lab control and station-protection boundaries.

- Confirm the repository, Git revision, status and authorized paths. Preserve
  existing user edits, fixtures, licensing and historical evidence.
- Follow the responsibility map in [Architecture](docs/en/architecture/overview.md).
  Keep portable logic independent of the native filesystem adapter. The crate
  forbids unsafe Rust. Preserve bounded parsing, opaque-byte preservation,
  portable paths, no-clobber publication and revision checks.
- Treat [spec/1.0](spec/1.0/README.md) as the canonical format source. Review a
  persisted change against the correct version domain, applicable schema,
  direct consumers and compatibility fixtures. Require an explicit version
  transition when semantics change incompatibly.
- Run the affected lanes in [Verification](docs/en/contributing/verification.md)
  from this repository root. Write generated evidence and test artifacts to
  allocated output locations. Record observed results and the tested revision.
- Keep English canonical in `docs/en/` and update the matching `docs/vi/`
  path in the same change. Review translation meaning before computing its
  English UTF-8/LF SHA-256. Put release changes in the appropriate major-version
  changelog and retain dated evidence in `docs/history/`.
- Use the Rules Task Continuity workflow for task-specific plans, working
  reports and artifact references. Keep durable format and architecture
  decisions with the owning specification or guide.
- Changes to another repository, tagging, publication, deployment and physical
  operations require separate authorization.
