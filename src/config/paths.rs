use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    base: PathBuf,
}

impl ConfigPaths {
    pub fn discover() -> Result<Self> {
        if let Some(path) = std::env::var_os("TORII_CONFIG_DIR").filter(|v| !v.is_empty()) {
            return Ok(Self::new(path.into()));
        }
        let base = dirs::home_dir()
            .ok_or(Error::NoConfigDirectory)?
            .join(".config")
            .join("torii");
        Ok(Self::new(base))
    }
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn providers(&self) -> PathBuf {
        self.base.join("providers")
    }
    pub fn settings(&self) -> PathBuf {
        self.base.join("settings.yaml")
    }
    pub fn log(&self) -> PathBuf {
        self.base.join("torii.log")
    }
    pub fn provider(&self, name: &str) -> ProviderPaths {
        ProviderPaths::new(self.providers().join(name))
    }
    pub fn ensure(&self) -> Result<()> {
        create_dir(&self.base)
    }
}

#[derive(Debug, Clone)]
pub struct ProviderPaths {
    base: PathBuf,
}

impl ProviderPaths {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn config(&self) -> PathBuf {
        self.base.join("provider.yaml")
    }
    pub fn rules(&self) -> PathBuf {
        self.base.join("rules.yaml")
    }
    pub fn env(&self) -> PathBuf {
        self.base.join(".env")
    }
    pub fn grants(&self) -> PathBuf {
        self.base.join("grants")
    }
    pub fn session_cache(&self) -> PathBuf {
        self.base.join(".session-cache")
    }
    pub fn auth_dir(&self) -> PathBuf {
        self.base.join("auth")
    }
    pub fn credentials(&self) -> PathBuf {
        self.auth_dir().join("credentials.env")
    }
    pub fn ensure(&self) -> Result<()> {
        create_dir(&self.base)?;
        create_dir(&self.auth_dir())
    }

    pub fn targets_dir(&self) -> PathBuf {
        self.base.join("targets")
    }

    pub fn target(&self, name: &str) -> TargetPaths {
        TargetPaths::new(self.targets_dir().join(name))
    }

    pub fn auth_paths(&self) -> AuthPaths {
        AuthPaths::new(self.base.clone())
    }
}

#[derive(Debug, Clone)]
pub struct TargetPaths {
    base: PathBuf,
}

impl TargetPaths {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn config(&self) -> PathBuf {
        self.base.join("target.yaml")
    }

    pub fn rules(&self) -> PathBuf {
        self.base.join("rules.yaml")
    }

    pub fn env(&self) -> PathBuf {
        self.base.join(".env")
    }

    pub fn grants(&self) -> PathBuf {
        self.base.join("grants")
    }

    pub fn ensure(&self) -> Result<()> {
        create_dir(&self.base)
    }
}

#[derive(Debug, Clone)]
pub struct AuthPaths {
    base: PathBuf,
}

impl AuthPaths {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn auth_dir(&self) -> PathBuf {
        self.base.join("auth")
    }

    pub fn credentials(&self) -> PathBuf {
        self.auth_dir().join("credentials.env")
    }

    pub fn session_cache(&self) -> PathBuf {
        self.base.join(".session-cache")
    }

    pub fn ensure(&self) -> Result<()> {
        create_dir(&self.auth_dir())
    }
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}
