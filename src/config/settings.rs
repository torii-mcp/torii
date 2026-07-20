use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const DEFAULT_GRANT_MINUTES: u32 = 2;
pub const DEFAULT_TARGET_MINUTES: u32 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default = "default_grant_minutes")]
    pub default_grant_minutes: u32,
    #[serde(default = "default_target_minutes")]
    pub default_target_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            default_grant_minutes: DEFAULT_GRANT_MINUTES,
            default_target_minutes: DEFAULT_TARGET_MINUTES,
        }
    }
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}
fn default_grant_minutes() -> u32 {
    DEFAULT_GRANT_MINUTES
}
fn default_target_minutes() -> u32 {
    DEFAULT_TARGET_MINUTES
}

pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let settings: Settings = serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    if !(1..=24 * 60).contains(&settings.default_target_minutes) {
        return Err(Error::InvalidArguments(format!(
            "default_target_minutes in {} must be between 1 and 1440",
            path.display()
        )));
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_receive_the_safe_target_default() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("settings.yaml");
        std::fs::write(&path, "max_output_bytes: 1024\ndefault_grant_minutes: 2\n").unwrap();
        assert_eq!(load(&path).unwrap().default_target_minutes, 15);
    }

    #[test]
    fn target_authorization_default_is_bounded() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("settings.yaml");
        std::fs::write(&path, "default_target_minutes: 0\n").unwrap();
        assert!(load(&path).is_err());
        std::fs::write(&path, "default_target_minutes: 1441\n").unwrap();
        assert!(load(&path).is_err());
    }
}
