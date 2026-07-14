use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const DEFAULT_GRANT_MINUTES: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default = "default_grant_minutes")]
    pub default_grant_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            default_grant_minutes: DEFAULT_GRANT_MINUTES,
        }
    }
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}
fn default_grant_minutes() -> u32 {
    DEFAULT_GRANT_MINUTES
}

pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}
