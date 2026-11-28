# CAD container architecture proposal 0.1

Status **proposed architecture**. This revision names responsibilities and
compatibility requirements for a future `.circad` container. The deployed
[Project Format 1.0 profile](../../../spec/1.0/README.md) is authoritative for
current manifests, envelopes and packages. A `.circad` file can be carried as
an opaque native asset under that profile. Editing, structural inspection
and migration of its inner format require a future CAD codec.

## Assembly and CAD ownership

| Representation | Authoritative content | Owner boundary |
| --- | --- | --- |
| `.cir3d` | Placement of referenced models, stable component instances, position and rotation, hierarchy, placement constraints, assembly configuration and optional model revision references | Assembly domain for project composition |
| Proposed `.circad` | Parametric design, sketches, constraints, feature timeline, authoritative geometry including native B-rep, CAD components and joints, parameters and expressions, configurations, materials and appearances | `cirnet-cad` for domain semantics, geometry, CAD codec and future SolidCraft adapter |
| `.cirpkg` | Portable project members, manifest and integrity inventory | Format Core for the outer project package |

An assembly references editable CAD geometry by resource identity and, when
needed, an exact model revision. The CAD owner defines CAD-local components
and joints. Project composition uses model references and placement rather
than embedding a second parametric model.

## Proposed storage responsibilities

```text
Model.circad
  container.json       container identity and version
  document.json        authored CAD design and required blob references
  blobs/               required SHA-256-addressed native payloads
  cache/
    preview.json       optional derived preview metadata
    preview.bin        optional derived display bytes
    model.json         optional derived model cache metadata
    bodies.bin         optional derived evaluated bodies
```

The layout is illustrative. The CAD owner chooses the payload schema, part
requirements and qualified encodings in the CAD Format Foundation. Distinguish
authored design metadata and required native source blobs from optional
evaluated geometry and previews. A source blob required to reconstruct the
design belongs with authored content, including when it contains geometry.
An invalid, outdated or missing optional cache is a cache miss after container
preflight and required-content validation. Missing or corrupt required bytes
block editable open. Cached meshes and previews supply only derived views.

Key required blobs by SHA-256 over their exact uncompressed bytes and qualify
how the encoding is identified. Use bounded streaming verification for required
data and ensure cache validity includes the source design identity and the
producer's cache version. A digest checks correspondence and corruption.
Authorization and trust require separate evidence.

## Version and codec boundaries

Keep CAD container profile, CAD domain schema, each blob encoding and each
optional cache revision independent from one another and from the existing
Project Format Manifest, Document Envelope and Package Profile 1.0.

Validate a known container separately from a known CAD schema. Expose
container inspection, structural validation, domain validation, reading,
writing, byte preservation and migration as separate capabilities. Unsupported
major versions may be carried exactly when the outer resource can be handled
safely. Enable edits only when the owner can validate the change and preserve
unknown content. Owner-registered migrations declare exact source and target
versions. Compatibility evidence should cover legacy design inputs, unknown
members, cache invalidation and unsupported future content.

Format Core's current registered document decoder supports JSON envelopes and
recognized legacy JSON. A filename extension or codec registry entry alone
cannot add ZIP parsing. A future representation adapter should honor these
requirements alongside the [transaction contract](transactions.md).

- Accept a bounded streaming or seekable source. Validate container identity,
  member paths and types, member counts, supported ZIP options, sizes and
  expanded budgets before materializing parts. Preserve outer package budgets
  independently from the CAD reader's inner budgets.
- Keep exact-byte hashing at the Format resource boundary. Snapshot digests
  establish read sets and optimistic revision checks. Stage authored output
  through the same no-clobber and manifest-last transaction policy with explicit
  owner codec validation. Recovery preserves divergent staged evidence.
- Verify a `.cirpkg` member at the outer package boundary. An opaque CAD
  member needs no inner interpretation for a byte-preserving transfer.
  Editable open separately verifies required CAD content and its domain schema.
- Route `.sc`, STEP and GLB interoperability through the CAD engine. Preserve
  editable parametric authority in the CAD document when exporting a derived
  result or importing an interchange representation.

## SolidCraft reference and reuse

Reviewed source revision `d94d83c2897df16607000e81e420b373bfa48ecc`. The
reference informs separated container, document, blob and cache versioning
and content-addressed storage. Its upstream `.sc` format and native B-rep
cache definitions remain upstream contracts.

- [Design container](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/docs/design-container.md)
- [Native B-rep](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/docs/native-brep.md)
- [Format crate](https://github.com/tangency-labs/solidcraft/tree/d94d83c2897df16607000e81e420b373bfa48ecc/crates/format)
- [Document version history](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/crates/doc/src/format.rs)

The reviewed upstream workspace declares MIT OR Apache-2.0 for code. Upstream
branding and specified icon artwork have separate terms. Before any reuse,
review the selected paths, license compatibility, copyright notices and asset
attribution with the owner. This proposal relies on documented architecture
and introduces no upstream source or artwork.

- [MIT license](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-MIT) · [Apache license](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-APACHE)
- [Brand licence](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-BRAND) · [Attribution](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/ATTRIBUTION.md) · [Third-party licenses](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/THIRD-PARTY-LICENSES.txt)
