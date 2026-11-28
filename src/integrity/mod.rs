mod digest;
#[cfg(feature = "native")]
pub(crate) use digest::transfer;
#[cfg(feature = "package")]
pub(crate) use digest::transfer_with;
pub use digest::{Digest, hash_reader};
