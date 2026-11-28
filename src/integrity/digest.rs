use crate::{
    Result,
    error::{check_limit, io},
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fmt,
    io::{Read, Write},
};

/// Lowercase hexadecimal SHA-256 over the exact input bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(serde::de::Error::custom(
                "expected 64 lowercase hexadecimal SHA-256 characters",
            ));
        }
        Ok(Self(value))
    }
}

/// Stream at most `max_bytes` bytes. Returns the exact byte count and its SHA-256.
pub fn hash_reader(reader: impl Read, max_bytes: u64) -> Result<(u64, Digest)> {
    transfer(reader, std::io::sink(), max_bytes, "input")
}

pub(crate) fn transfer(
    reader: impl Read,
    writer: impl Write,
    limit: u64,
    context: &str,
) -> Result<(u64, Digest)> {
    transfer_with(reader, writer, limit, context, |_| Ok(()))
}

pub(crate) fn transfer_with(
    mut reader: impl Read,
    mut writer: impl Write,
    limit: u64,
    context: &str,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<(u64, Digest)> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut count = 0u64;
    loop {
        let capacity = limit
            .saturating_sub(count)
            .saturating_add(1)
            .min(buffer.len() as u64) as usize;
        let read = match reader.read(&mut buffer[..capacity]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(|error| io(context, error))?,
        };
        if read == 0 {
            break;
        }
        count = count.saturating_add(read as u64);
        check_limit("stream bytes", count, limit)?;
        writer
            .write_all(&buffer[..read])
            .map_err(|error| io(context, error))?;
        hasher.update(&buffer[..read]);
        progress(count)?;
    }
    Ok((count, Digest(format!("{:x}", hasher.finalize()))))
}
