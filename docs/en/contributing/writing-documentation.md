# Write and maintain Project Format documentation

[Tiếng Việt](../../vi/contributing/writing-documentation.md) · [Documentation](../index.md)

Follow the [repository instructions](../../../AGENTS.md) and the CIRNET Rules
[documentation policy](https://github.com/cirnetlab/cirnet-rules/blob/master/en/policies/documentation.md).
Search existing topics and incoming links before adding a page. Put each
procedure or explanation with its owning document and link to it from indexes.

## Choose the owning location

| Location | Purpose |
| --- | --- |
| Root [README](../../../README.md) | Introduce the crate and link both language indexes. |
| `docs/en/` and `docs/vi/` | Keep current architecture, user guides and contributor procedures in mirrored paths. |
| [Versioned specification and schemas](../../../spec/1.0/README.md) | Define the persisted format. Follow the version and schema policy when changing semantics. |
| [Changelog index](../../changelogs/README.md) | Maintain shared English `CHANGELOG_V<major>.md` files, one per major when needed. |
| [Historical evidence](../../history/v0.1.0-verification.md) | Preserve the original language, source revision, results, artifact digests and scope. |

The Core owns project serialization and packaging. Cite CIRNET Rules for
organization-wide architecture and the relevant domain owner for its
engineering semantics. Keep task-specific plans and progress in
[Task Continuity](https://github.com/cirnetlab/cirnet-rules/blob/master/en/docs/distribution.md#task-continuity).

Before moving or merging a page, check incoming links and any references in
source, tests, build configuration and CI. Retain unique commands, requirements,
limits and provenance. Use relative paths for internal links and a full commit
permalink for exact cross-repository historical evidence.

## Update the English and Vietnamese pair

Use lowercase-with-dashes filenames for current guides, one H1, descriptive
headings and a language identifier on code fences. Write canonical English
under `docs/en/` and translate to the same relative path under `docs/vi/`.
Keep paths, commands, API identifiers, versions and verification statuses
exact. Review technical meaning before updating the translation footer.
Use the repo-relative English source path for `source` and the lowercase
SHA-256 of the exact UTF-8/LF English bytes for `source-sha256`.

## Record changes and verify

Record pending changes under `Unreleased`. List full SemVer milestones newest
first. Use the applicable `Added`, `Changed`, `Fixed`, `Deprecated`,
`Removed` and `Security` categories, omitting empty categories. Cite the
version manifest and source revision. Give a publication date only when
verified and record the actual status of development candidates.

Review both languages, internal paths and anchors, then run the
[documentation checks](verification.md#documentation-checks).
