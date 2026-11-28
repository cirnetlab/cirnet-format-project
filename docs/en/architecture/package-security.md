# Package security

A portable package uses the [1.0 ZIP32 profile](../../../spec/1.0/README.md#portable-zip)
and contains the project manifest, registered local resources and one SHA-256
inventory. Native engineering assets remain in their original encodings.

| Operation | Result |
| --- | --- |
| Inspect | Manifest, its integrity and declared archive membership |
| Verify | Checks member data and document containers without publishing |
| Prepare verified import | Writes verified member bytes to caller-controlled scratch storage |
| Publish | Creates a new destination from prepared data and publishes its manifest last |
| Export | Writes the manifest and selected project resources to a package sink |

Preflight checks local and central records, member names, overlap, size and
compression bounds before reading payloads. Verify checks exact membership,
byte counts and streamed SHA-256. Valid JSON and envelopes still require domain
validation for editing. Publisher identity, permission to disclose or import,
permission to execute and physical execution safety remain separate controls.
Imported source, firmware, scripts and configurations are inert assets.

Report inspection as metadata with resource verification pending. The
`*_with_progress` APIs call back with `PackageProgress`, including phase,
optional member name and current member byte count. Return
`ControlFlow::Break(())` to cancel at a checkpoint. Direct export to a caller
sink can leave partial bytes in that sink. A canceled native file export
keeps the target unpublished. Use private, quota-controlled import staging.
Use `VerifiedPackage::publish_retryable` to keep the prepared import after a
clean pre-publication rejection, including cancellation. It removes only
verified files installed by the operation and empty directories it created.
`PackagePublishError::Incomplete` reports a retained destination when
cleanup is uncertain. Do not delete retained entries without establishing
ownership. After interruption, use `inspect_package_destination` to check
for a pending import marker or a manifest. The marker is diagnostic and
supplies no authority to delete a destination after a restart. Reserve an
exclusive, caller-controlled local destination during publication and rollback.
A manifest indicates the publication point. A published outcome carries the opened project and any
synchronization or cleanup warning.

## Portable paths and future profiles

The 1.0 path profile uses ASCII components and excludes hidden segments, device
names and case aliases. Use safe registered paths in the working directory and
packages. Keep credentials, private files and machine-local locators outside
portable membership. Treat external coordinates as declarations, and resolve
or copy them only through an authorized consumer.

Adding Unicode would require an explicit compatibility profile covering
normalization, stable spelling, case equivalence, archive encoding and path
length across platforms. Source trees that need dotfiles require a separate
versioned policy with an allowlist and checks for secrets and private paths.
Test migration, existing path collisions and cross-platform round trips
before enabling either. Private and external locators require an explicit
trust and access model. They have no automatic resolution in 1.0.
