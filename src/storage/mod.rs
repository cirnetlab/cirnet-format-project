//! Native filesystem adapter for working projects.
mod native;
pub use native::{Project, Snapshot, StagedFile};
pub(crate) use native::{
    ensure_manifest_path, no_link_metadata, open_regular, persist_new, publish_new_resource,
    read_bounded, read_file, replace_staged, replace_temp, resolve, sync_dir, temp_bytes,
    trusted_root, write_lock,
};
