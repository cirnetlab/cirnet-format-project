/// Format failures include a stable category and a location or operation where available.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid JSON at byte {offset}: {reason}")]
    JsonPolicy { offset: usize, reason: &'static str },
    #[error("{resource} exceeds limit {limit}")]
    Limit { resource: &'static str, limit: u64 },
    #[error("unsupported {domain} format/version: {version}")]
    Unsupported {
        domain: &'static str,
        version: String,
    },
    #[error("invalid project path: {0}")]
    Path(String),
    #[error("project validation failed: {0}")]
    Validation(String),
    #[error("missing resource: {0}")]
    Missing(String),
    #[error("content conflict at {0}")]
    Conflict(String),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("invalid portable ZIP: {0}")]
    Package(String),
    #[error("{context} was installed, but directory durability could not be confirmed: {source}")]
    Durability {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error for {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Error {
    Error::Io {
        context: context.into(),
        source,
    }
}

pub(crate) fn check_limit(resource: &'static str, size: u64, limit: u64) -> Result<()> {
    if size > limit {
        return Err(Error::Limit { resource, limit });
    }
    Ok(())
}
