# Verification report

Baseline `3db3d10af12e8207ff99c505cc19c6f2c30ddc42` plus the local Project Format Foundation changes. Scope is the standalone format core. Recorded on 2026-11-28 with Rust 1.99.1, x86_64-pc-windows-msvc, Windows build 26300 and an Intel Core i5-12500H with 16 logical processors.

| Check | Result |
| --- | --- |
| Format and Clippy with all features and with no default features | PASS |
| Unit, integration and doc tests on Windows | PASS, 13 checks in the regular native run and one Windows junction check run separately |
| Byte-oriented configuration with no default features | PASS, 6 integration tests and 1 doctest |
| Release builds with both feature configurations | PASS |
| Rust API documentation with warnings denied | PASS |
| Draft 2020-12 schemas checked against the generated manifest, envelope and package inventory | PASS |
| Working-project creation and package example | PASS |
| Contracts validation and bit-for-bit legacy preservation | PASS for CircuitGraph 1.0, 1.1 and 1.2 |
| Advisory queries for the locked dependencies | PASS, no matching OSV record returned for the 55 locked crates.io packages at verification time |
| Windows release benchmarks | PASS, observed results below |
| Linux, macOS, Rust 1.89 qualification, and WASM | NOT_RUN |
| UI, Remote and CLI integration, published release or hardware acceptance | NOT_RUN |

The Contracts check used the source at `1d092e2ab061537d8bdec3ec020ff03e779fe11f` and local Contracts CLI with SHA-256 `7cb778799ef4c148d652afa443b40c475f889c037653af224a6322123da63458`. The sample `Lab.cirpkg` has SHA-256 `c812489be349698cc57e3887e4e5bb5b0c64d4b1650b7e83bb85ad81597f81e1`.

The directly selected crates use MIT or Apache-2.0 options, including serde, serde_json, sha2, thiserror and tempfile. ZIP 9.8.2 uses MIT and its selected pure-Rust compression backend uses the Zlib license. ZIP defaults are disabled. The archive reader uses the Stored and DEFLATE methods and its own verified staging. Existing repository license notices remain in place. See the [ZIP crate metadata](https://docs.rs/crate/zip/9.8.2) and the [OSV query service](https://osv.dev/).

## Release benchmarks

Run `cargo bench --locked --bench workloads`. The workload uses one warm-up and seven measured samples per scenario. It generates approximately 2 KiB per authored JSON document and one fixed-seed native binary asset per scenario. Measurements include local Windows filesystem overhead and warm caches. ZIP throughput uses archive bytes divided by mean wall-clock time. Verify includes staging and per-file syncs. With seven samples the nearest-rank P95 is the largest observation.

| Workload | Documents | Native asset | Manifest bytes | ZIP bytes |
| --- | ---: | ---: | ---: | ---: |
| Small | 20 | 1 MiB | 4,412 | 1,057,703 |
| Medium | 200 | 16 MiB | 41,252 | 16,860,307 |
| Large | 1,000 | 96 MiB | 206,052 | 101,076,377 |

Latency values are minimum / median / P95 milliseconds.

| Operation | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| Manifest open | 0.322 / 0.333 / 0.571 | 1.057 / 1.130 / 1.852 | 4.558 / 10.123 / 15.547 |
| Document read | 0.099 / 0.117 / 0.253 | 0.137 / 0.156 / 0.277 | 0.205 / 0.419 / 0.891 |
| Document save | 1.159 / 1.395 / 2.879 | 1.244 / 2.105 / 2.847 | 3.773 / 7.262 / 10.900 |
| Indexed lookup batch (100,000) | 1.607 / 1.624 / 1.986 | 1.638 / 1.693 / 6.527 | 1.839 / 4.571 / 8.805 |
| ZIP export | 5.872 / 7.541 / 16.605 | 46.309 / 49.795 / 95.791 | 292.754 / 672.377 / 732.847 |
| ZIP verify and stage | 61.843 / 77.630 / 89.191 | 543.432 / 696.943 / 732.404 | 4375.788 / 5261.186 / 9049.300 |
| SHA-256 | 0.672 / 0.772 / 0.930 | 11.388 / 12.542 / 17.145 | 85.087 / 108.707 / 119.032 |

| Throughput (MiB/s) | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| ZIP export | 105.7 | 258.5 | 166.8 |
| ZIP verify and stage | 13.2 | 23.8 | 15.1 |
| SHA-256 | 1259.9 | 1227.7 | 908.7 |

The observed peak working set for the full Windows benchmark process was 14,422,016 bytes (13.75 MiB). This figure is a process metric sampled from the Windows peak counter, including runtime overhead. Workloads are synthetic. Results depend on storage, cache state and sync behavior.
