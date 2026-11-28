# CIRNET Project Format 1.0

Status **candidate**. Rust crate version, manifest version, envelope version, domain schema version, specification of shared Contracts, and package version evolve independently.

| Extension | Representation | Core capability |
| --- | --- | --- |
| `.cirproj` | UTF-8 JSON manifest | Read, validate, write |
| `.cirsch` | UTF-8 JSON envelope, kind `schematic` | Container only |
| `.cirpcb` | UTF-8 JSON envelope, kind `pcb` | Container only |
| `.cir3d` | UTF-8 JSON envelope, kind `assembly` | Container and stable asset links |
| `.cirlib` | UTF-8 JSON envelope, kind `library` | Container and stable asset links |
| `.cirpkg` | ZIP32 profile | Inspect, verify, write, stage, and publish |

[Manifest schema](manifest.schema.json) · [Document schema](document.schema.json) · [Integrity schema](integrity.schema.json). The Rust core also checks portable path constraints, resource budgets, unique IDs, cross-record relationships, namespace rules, and dependency cycles.

## Manifest and documents

A working project opens from one chosen root-level `.cirproj` file. Directories are optional and follow the registry, such as `documents/`, `libraries/`, `assets/`, `configurations/` or `results/`. The manifest contains display metadata, domains, document and asset registries, optional library coordinates, links, and extension data. Editors may derive their own tree projections from this metadata without loading model bytes.

Project and resource IDs are caller-assigned, stable across relocation, and case-sensitive. The registry uses one ID namespace for documents, assets, and external libraries. Paths have an independent case-alias check. References target IDs. `reference`, `configuration` and `result` may form cycles. `dependency` and `library` edges between documents must be acyclic. Library links point to local library documents or external library coordinates. Configuration and result links point to a local resource. A required external library must be resolved into project membership before packaging. Optional external declarations stay as metadata and trigger no network calls.

The envelope has `format: "cirnet.document"`, `format_version`, `id`, `kind`, `schema_version`, `payload` and optional dotted namespaces in `extensions`. For a recognized 1.0 envelope, metadata must agree with the registry. Unknown kind or envelope versions remain byte-preservable and have no editing or execution capability. Existing CircuitGraph files are the explicit exception, registered as kind `circuit-graph` with a `.json` locator. The registry ID and the graph's authored `circuit_id` are distinct. The Contracts validator approves the original bytes before a legacy import can create a registry entry. The original graph file remains an original graph file, with all precision, properties, metadata and presentation bytes intact.

## Deterministic JSON

Inputs are UTF-8 without BOM, one JSON value, with unique decoded keys. Writers use lexicographic object-key order, keep array order, escape using JSON rules, use two-space indentation and emit a final LF. Members whose optional presence carries meaning are emitted or omitted by their schema. Source preservation never relies on canonical reserialization of a legacy or opaque document.

IDs use ASCII letters, digits, dots, hyphens and underscores, start alphanumeric and are at most 128 bytes. Paths are slash relative, use ASCII letters, digits, internal spaces, hyphens, dots and underscores, with up to 96 bytes per component and 240 bytes total. Hidden segments, trailing dots or spaces, Windows device stems, case aliases and file/directory collisions are invalid. The package reserves the root names `project.cirproj` and `integrity.json` and their case aliases.

Owned integer values meant for JSON number interop stay within JavaScript's safe integer interval (±9,007,199,254,740,991). Actual ZIP32 byte counts are smaller. Domain payloads can retain longer integers and precise decimals with the lossless Rust JSON codec. JS consumers must use a lossless codec for such payloads and may not coerce numbers through binary64. The core rejects invalid number grammar, more than 128 characters per numeric token, nonfinite binary64 magnitude and nonzero underflow to zero. Units belong to domain schemas. New interchange timestamp fields should use RFC 3339 UTC strings with an explicit `Z`. This envelope defines no timestamp or unit fields.

The core recognizes context-specific reserved credential and authority key names, including password, access/refresh/CSRF/session tokens, authorization, private keys, execution leases, hardware permits and safety approval. Keys are compared after stripping ASCII punctuation and folding ASCII case. Consumers must select authorized authored content and exclude secrets hidden in native assets or invented field names. Native assets retain their own formats and are treated as opaque bytes.

## Portable ZIP

Single disk, ZIP32, no outer ZIP comment, plaintext regular-file entries only, Stored or DEFLATE. Archive paths equal project-relative registry paths. Export order is manifest and resources in lexicographic path order, followed by `integrity.json`. The ZIP library writes fixed default DOS timestamps and normalized regular permissions. Optional DEFLATE level 6 applies to authored JSON files when the expansion limit can accommodate it. Native assets and the inventory are stored. Reproducibility is byte-for-byte with the same crate lock, inputs, order, and compression policy. Readers accept any member order and never interpret extraction attributes as permissions.

The ZIP includes one `project.cirproj`, all registered local documents and native assets, and one `integrity.json`. No directory entries, data descriptors, duplicate names, overlapping entry ranges, undeclared data gaps or unrelated files. Local and central headers must agree on name, flags, method, CRC and sizes. Membership is explicit. A file present only in the working directory stays outside the archive. The inventory identifies `cirnet.package` version `1.0`, algorithm `sha256` and an object keyed by member path. Each item states the exact uncompressed byte count and the lowercase hexadecimal SHA-256 of those exact bytes. The inventory excludes itself. SHA-256 verifies content consistency and carries no publisher identity or authorization. Inspect checks manifest integrity and declared membership. Verify streams every member and validates document/container headers before an import destination is touched.

## Defaults and failures

| Limit | Default |
| --- | --- |
| JSON input | 8 MiB, depth 64, 500,000 nodes including keys |
| Registry | 8,190 resources, 65,536 relationships |
| Native asset | 512 MiB |
| ZIP total expanded | 2 GiB including manifest and inventory |
| ZIP file | 2 GiB + 16 MiB, constrained further by ZIP32 |
| ZIP central directory | 8 MiB |
| ZIP expansion ratio | 2,048:1 per member |

The caller can lower limits or raise them within hard interoperability bounds. Diagnostics distinguish unsupported versions, invalid paths, missing resources, conflicts, configured limits, package structure, integrity and I/O errors. A message describes context without exposing a credential value. Manifest format versions other than 1.0 require a separate reader or explicit migration. The migration API validates each owner-supplied version transition. The core carries no implicit historical migration and never overwrites the input of a migration.
