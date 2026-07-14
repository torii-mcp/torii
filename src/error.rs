use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not determine the user's configuration directory")]
    NoConfigDirectory,
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid YAML in {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("invalid provider {provider}: {reason}")]
    InvalidProvider { provider: String, reason: String },
    #[error("duplicate provider tool name {0:?}")]
    DuplicateTool(String),
    #[error("duplicate provider name {0:?}")]
    DuplicateProviderName(String),
    #[error("provider tool {0:?} is not installed")]
    ProviderNotFound(String),
    #[error("rules file not found at {0}")]
    RulesNotFound(PathBuf),
    #[error("invalid env file {path}: {reason}")]
    EnvParse { path: PathBuf, reason: String },
    #[error("failed to launch {program:?}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("provider {provider:?} has no valid session; run `torii reauth {provider}`")]
    SessionInvalid { provider: String },
    #[error("authentication for provider {0:?} was cancelled")]
    AuthCancelled(String),
    #[error("authentication strategy {strategy:?} is not implemented for provider {provider:?}")]
    AuthStrategyNotImplemented { provider: String, strategy: String },
    #[error("invalid MCP tool arguments: {0}")]
    InvalidArguments(String),
    #[error("MCP server failed: {0}")]
    Mcp(String),
    #[error("GUI prompt failed: {0}")]
    Prompt(String),
    #[error("provider package error: {0}")]
    Package(String),
    #[error("agent integration error: {0}")]
    Agent(String),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        1
    }
}

pub type Result<T> = std::result::Result<T, Error>;
