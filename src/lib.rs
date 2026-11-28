//! CIRNET LAB project manifests, document envelopes, and portable packages.
#![forbid(unsafe_code)]

mod document;
mod error;
mod integrity;
mod json;
mod manifest;
mod migration;
mod path;
pub use document::*;
pub use error::{Error, Result};
pub use integrity::{Digest, hash_reader};
pub use manifest::*;
pub use migration::*;
pub use path::ProjectPath;

#[cfg(feature = "native")]
pub mod package;
#[cfg(feature = "native")]
pub mod storage;

/// Limits are applied at each untrusted input boundary. Lower them for constrained hosts.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_json_bytes: u64,
    pub max_json_depth: usize,
    pub max_json_nodes: usize,
    pub max_resources: usize,
    pub max_relationships: usize,
    pub max_asset_bytes: u64,
    pub max_total_bytes: u64,
    pub max_archive_bytes: u64,
    pub max_directory_bytes: u64,
    pub max_compression_ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_json_bytes: 8 * 1024 * 1024,
            max_json_depth: 64,
            max_json_nodes: 500_000,
            max_resources: 8190,
            max_relationships: 65_536,
            max_asset_bytes: 512 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_archive_bytes: 2 * 1024 * 1024 * 1024 + 16 * 1024 * 1024,
            max_directory_bytes: 8 * 1024 * 1024,
            max_compression_ratio: 2048,
        }
    }
}
