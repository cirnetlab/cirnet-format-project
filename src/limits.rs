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
