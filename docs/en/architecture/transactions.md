# Transactions and recovery

A native working project carries a manifest revision digest. Read exact
document bytes with `Project::read_document` and use the revision returned
with that read. Use `Project::resource_revision` for other local resources.
Build a `ChangeSet` with the expected manifest digest and pass affected-file
digests and declared read dependencies to `ChangeSet::expect`. Capture the
tokens used to make the edit. Include the existing local resources in the
hard dependency closure of each affected resource. A document-only save
can leave the manifest digest stable. Resource revision checks catch
those stale snapshots.

| Operation | Intent |
| --- | --- |
| Create | Register a new resource and place bytes at an unused path |
| Replace | Change an existing resource after checking its expected digest |
| Relocate | Carry unchanged bytes to a new locator for the same resource ID |
| Migrate | Apply an explicit supported transition and validate its output |
| Remove | Drop registry membership deliberately after checking impact. The previous file remains for explicit cleanup. |

Read requests can remain lazy. Transaction work scales with the affected
resources and declared reads. Structural validation applies to persisted
metadata and document envelopes. An authored document change requires
domain-owned validation. Preserve unsupported document bytes on relocation,
and retain unknown content during edits to supported documents.

## Publication and recovery

For a manifest-changing transaction, stage new files at unused paths and
publish the new manifest last. The old manifest can still address the old
resources until that point. A document-only save replaces one file atomically
through the same revision and validation policy. It syncs a temporary file
in the document's directory before replacing the destination. Only
manifest-changing transactions need a journal. An interruption during a
single-file save leaves the destination at its old or new revision and can
leave an unused temporary file in that directory.

| Outcome | Caller action |
| --- | --- |
| Rejected before publication | Keep the existing state and handle the conflict, validation error or I/O failure |
| Committed | Read from the committed snapshot |
| Published with `Error::Durability` | Reopen and inspect the reported path before retrying |
| Published with `Error::PublishedCleanup` | Keep the published result and explicitly recover the local staging |
| Interrupted | Inspect the local recovery record and explicitly recover the transaction |

`Project::inspect_recovery` examines the bounded journal in the private
`.cirnet-transaction` directory. `Project::recover` clears private staging
after a staging interruption or when the publication point matches the
journal. It reports installed, unreferenced files for caller review and
retains them when ownership is uncertain. A diverged publication point
preserves the journal and files for caller review. Establish filesystem
ownership before removing a reported path.
Use the project lock to coordinate writers through the library. Use a trusted
local working directory, and keep its path components under caller control.
Symlinks and Windows reparse points are rejected by the native adapter where
detected. A process that can concurrently replace inspected path components
requires stronger isolation from the host.

File and directory synchronization have platform and filesystem limits. Treat
power-loss durability and shared or network filesystems as separate
qualification questions. Reopen the manifest and affected files after an
ambiguous failure before issuing another write.
