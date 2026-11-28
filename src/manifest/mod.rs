mod codec;
mod model;
mod validation;
pub use codec::decode_manifest;
pub use model::{
    AssetEntry, DocumentEntry, Extensions, ExternalLibrary, FORMAT_VERSION, MANIFEST_FORMAT,
    MANIFEST_FORMAT_VERSION, Manifest,
};
pub(crate) use validation::{check_extensions, check_token, check_version};
