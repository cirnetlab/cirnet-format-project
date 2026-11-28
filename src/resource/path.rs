use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, fmt, path::Path};

/// A portable, case-sensitive locator. Case aliases are forbidden within a project.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProjectPath(String);

impl ProjectPath {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        if !(1..=240).contains(&path.len()) || path.split('/').any(invalid_segment) {
            return Err(Error::Path(path));
        }
        Ok(Self(path))
    }

    /// Returns the original portable spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
    pub fn extension(&self) -> Option<&str> {
        self.as_path().extension()?.to_str()
    }
}

fn invalid_segment(segment: &str) -> bool {
    if !(1..=96).contains(&segment.len())
        || segment.starts_with('.')
        || segment.ends_with(['.', ' '])
        || !segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b' '))
    {
        return true;
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
}

impl<'de> Deserialize<'de> for ProjectPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        ProjectPath::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(crate) fn unique_paths<'a>(paths: impl IntoIterator<Item = &'a ProjectPath>) -> Result<()> {
    let mut seen = BTreeMap::new();
    for path in paths {
        let mut boundary = 0;
        for segment in path.as_str().split('/') {
            boundary += segment.len();
            let spelling = &path.as_str()[..boundary];
            let file = boundary == path.as_str().len();
            match seen.entry(spelling.to_ascii_lowercase()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((spelling, file));
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    let (previous, previous_file) = *entry.get();
                    if previous != spelling || file || previous_file {
                        return Err(Error::Conflict(format!("{previous} and {spelling}")));
                    }
                }
            }
            boundary += 1;
        }
    }
    Ok(())
}
