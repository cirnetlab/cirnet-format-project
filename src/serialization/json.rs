use crate::error::check_limit;
use crate::{Error, Limits, Result};
use serde::{
    Serialize,
    de::{DeserializeOwned, IgnoredAny},
};
use std::{borrow::Cow, collections::HashSet, ops::Range};

/// Preflight before serde's Value parser, which otherwise accepts duplicate keys.
/// Object keys are decoded for comparisons, including escaped spellings.
pub(crate) fn read<T: DeserializeOwned>(bytes: &[u8], limits: &Limits) -> Result<T> {
    preflight(bytes, limits)?;
    Ok(serde_json::from_slice(bytes)?)
}

fn preflight(bytes: &[u8], limits: &Limits) -> Result<()> {
    check_limit("JSON bytes", bytes.len() as u64, limits.max_json_bytes)?;
    std::str::from_utf8(bytes).map_err(|error| Error::JsonPolicy {
        offset: error.valid_up_to(),
        reason: "expected valid UTF-8",
    })?;
    let mut scanner = Scanner {
        bytes,
        pos: 0,
        nodes: 0,
        limits,
    };
    scanner.value(1)?;
    scanner.space();
    if scanner.pos != bytes.len() {
        return Err(scanner.error("trailing input"));
    }
    Ok(())
}

pub(crate) fn write<T: Serialize>(value: &T, limits: &Limits) -> Result<Vec<u8>> {
    let mut object = serde_json::to_value(value)?;
    object.sort_all_objects();
    let mut bytes = serde_json::to_vec_pretty(&object)?;
    bytes.push(b'\n');
    preflight(&bytes, limits)?;
    Ok(bytes)
}

struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
    limits: &'a Limits,
}

struct StringToken {
    range: Range<usize>,
    escaped: bool,
}

impl<'a> Scanner<'a> {
    fn error(&self, reason: &'static str) -> Error {
        Error::JsonPolicy {
            offset: self.pos,
            reason,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        if self.peek() != Some(byte) {
            return false;
        }
        self.pos += 1;
        true
    }
    fn tick(&mut self) -> Result<()> {
        self.nodes += 1;
        check_limit(
            "JSON nodes",
            self.nodes as u64,
            self.limits.max_json_nodes as u64,
        )
    }
    fn string_token(&mut self) -> Result<StringToken> {
        let start = self.pos;
        let mut escaped = false;
        if !self.take(b'"') {
            return Err(self.error("expected a JSON string"));
        }
        while let Some(byte) = self.peek() {
            self.pos += 1;
            match byte {
                b'"' => {
                    return Ok(StringToken {
                        range: start..self.pos,
                        escaped,
                    });
                }
                b'\\' => {
                    escaped = true;
                    if self.peek().is_none() {
                        break;
                    }
                    self.pos += 1;
                }
                b'\0'..=b'\x1f' => return Err(self.error("control character in JSON string")),
                _ => {}
            }
        }
        Err(self.error("unterminated JSON string"))
    }

    fn decoded_key(&mut self) -> Result<Cow<'a, str>> {
        let StringToken { range, escaped } = self.string_token()?;
        if escaped {
            return Ok(Cow::Owned(serde_json::from_slice(&self.bytes[range])?));
        }
        Ok(Cow::Borrowed(
            std::str::from_utf8(&self.bytes[range.start + 1..range.end - 1])
                .expect("input was validated as UTF-8"),
        ))
    }
    fn value(&mut self, depth: usize) -> Result<()> {
        self.space();
        self.tick()?;
        check_limit(
            "JSON depth",
            depth as u64,
            self.limits.max_json_depth.min(120) as u64,
        )?;
        match self.peek() {
            Some(b'{') => self.container(depth, true),
            Some(b'[') => self.container(depth, false),
            Some(b'"') => {
                let StringToken { range, escaped } = self.string_token()?;
                if escaped {
                    serde_json::from_slice::<String>(&self.bytes[range])?;
                }
                Ok(())
            }
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a JSON value")),
        }
    }
    fn literal(&mut self, word: &[u8]) -> Result<()> {
        if !self.bytes[self.pos..].starts_with(word) {
            return Err(self.error("invalid JSON literal"));
        }
        self.pos += word.len();
        Ok(())
    }
    fn number(&mut self) -> Result<()> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E')
        ) {
            self.pos += 1;
            check_limit("number length", (self.pos - start) as u64, 128)?;
        }
        let value = &self.bytes[start..self.pos];
        // Check JSON number grammar without allocating an arbitrary-precision Number.
        serde_json::from_slice::<IgnoredAny>(value)?;
        let text = std::str::from_utf8(value).expect("scanner accepts only ASCII number bytes");
        let machine = text
            .parse::<f64>()
            .map_err(|_| self.error("invalid number"))?;
        let insignificant = text
            .split(['e', 'E'])
            .next()
            .unwrap_or("")
            .bytes()
            .all(|byte| !matches!(byte, b'1'..=b'9'));
        if !machine.is_finite() || (!insignificant && machine == 0.0) {
            return Err(self.error("number outside the finite binary64 magnitude range"));
        }
        Ok(())
    }
    fn container(&mut self, depth: usize, object: bool) -> Result<()> {
        self.pos += 1;
        let close = if object { b'}' } else { b']' };
        let mut keys = HashSet::new();
        self.space();
        if self.take(close) {
            return Ok(());
        }
        loop {
            if object {
                self.tick()?;
                let key = self.decoded_key()?;
                // serde_json reserves this key in arbitrary-precision mode.
                if key == "$serde_json::private::Number" {
                    return Err(self.error("reserved JSON key"));
                }
                if forbidden_key(&key) {
                    return Err(self.error("session credential or execution authority field"));
                }
                if !keys.insert(key) {
                    return Err(self.error("duplicate object key"));
                }
                self.space();
                if !self.take(b':') {
                    return Err(self.error("expected colon"));
                }
            }
            self.value(depth + 1)?;
            self.space();
            if self.take(close) {
                return Ok(());
            }
            if !self.take(b',') {
                return Err(self.error("expected comma or closing delimiter"));
            }
            self.space();
        }
    }
}

fn forbidden_key(key: &str) -> bool {
    // A longer normalized key cannot match any reserved name.
    let mut normalized = [0; 16];
    let mut length = 0;
    for byte in key.bytes().filter(u8::is_ascii_alphanumeric) {
        let Some(slot) = normalized.get_mut(length) else {
            return false;
        };
        *slot = byte.to_ascii_lowercase();
        length += 1;
    }
    matches!(
        &normalized[..length],
        b"accesstoken"
            | b"refreshtoken"
            | b"csrftoken"
            | b"sessioncookie"
            | b"sessiontoken"
            | b"authorization"
            | b"password"
            | b"privatekey"
            | b"executionlease"
            | b"leasetoken"
            | b"hardwarepermit"
            | b"safetyapproval"
    )
}
