use super::appearance::{FoldedBarColor, Theme};
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
    /// Theme of the prompt windows.
    #[serde(default)]
    pub theme: Theme,
    /// Color of a prompt window's title bar while the window is folded.
    #[serde(default)]
    pub folded_bar_color: FoldedBarColor,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            default_grant_minutes: DEFAULT_GRANT_MINUTES,
            default_target_minutes: DEFAULT_TARGET_MINUTES,
            theme: Theme::default(),
            folded_bar_color: FoldedBarColor::default(),
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

/// Write every setting, so the file also documents the values left at their
/// default.
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let contents = serde_yaml::to_string(settings)
        .map_err(|error| Error::InvalidSetting(format!("cannot serialize settings: {error}")))?;
    std::fs::write(path, contents).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
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

    #[test]
    fn old_settings_receive_the_default_appearance() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("settings.yaml");
        std::fs::write(&path, "max_output_bytes: 1024\n").unwrap();
        let settings = load(&path).unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.folded_bar_color, FoldedBarColor::default());
    }

    #[test]
    fn appearance_is_saved_as_plain_names() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("settings.yaml");
        let settings = Settings {
            theme: Theme::Nord,
            folded_bar_color: FoldedBarColor::Custom([1, 2, 3]),
            ..Settings::default()
        };
        save(&path, &settings).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("theme: nord"));
        assert!(contents.contains("folded_bar_color: '#010203'"));
        let reloaded = load(&path).unwrap();
        assert_eq!(reloaded.theme, Theme::Nord);
        assert_eq!(reloaded.folded_bar_color, FoldedBarColor::Custom([1, 2, 3]));
    }

    #[test]
    fn an_unknown_appearance_is_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("settings.yaml");
        std::fs::write(&path, "theme: solarized\n").unwrap();
        assert!(load(&path).is_err());
        std::fs::write(&path, "folded_bar_color: roxo\n").unwrap();
        assert!(load(&path).is_err());
    }
}
