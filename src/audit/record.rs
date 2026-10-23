//! What one audit record holds.
//!
//! A record is the whole story of one MCP call, or of one human control-plane
//! action. The database file is encrypted whole, so none of it reads without
//! the machine's audit key: not even when or what.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// An MCP call made by an agent.
    Call,
    /// A human command that changed state: a target lease, a reauth, a policy
    /// or settings edit.
    Control,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Call => "call",
            Kind::Control => "control",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        [Kind::Call, Kind::Control]
            .into_iter()
            .find(|kind| kind.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Begun and not finished: still running, or the process died mid-call.
    Running,
    Allowed,
    Denied,
    /// Failed before a decision or while executing.
    Error,
    /// A control-plane action, which has no verdict.
    Done,
}

impl Outcome {
    const ALL: [Outcome; 5] = [
        Outcome::Running,
        Outcome::Allowed,
        Outcome::Denied,
        Outcome::Error,
        Outcome::Done,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Outcome::Running => "running",
            Outcome::Allowed => "allowed",
            Outcome::Denied => "denied",
            Outcome::Error => "error",
            Outcome::Done => "done",
        }
    }

    pub fn parse(name: &str) -> Option<Outcome> {
        Outcome::ALL
            .into_iter()
            .find(|outcome| outcome.name() == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub kind: Kind,
    /// Unix milliseconds.
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// The provider tool the agent called, or the provider a human acted on.
    pub tool: String,
    pub target: Option<String>,
    /// `tool` or `tool/target`: the scope the events of the call belong to.
    pub scope: String,
    /// The argv exactly as the agent sent it.
    pub args: Vec<String>,
    /// The agent's own, unverified account of why it makes the call.
    pub description: Option<String>,
    pub outcome: Outcome,
    /// How the verdict was reached: `rules`, `grant`, `human-once`,
    /// `explicit-deny`, … For a control record, the action.
    pub source: Option<String>,
    /// The rule, grant or policy line behind the verdict.
    pub rule: Option<String>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout_bytes: Option<u64>,
    pub stderr_bytes: Option<u64>,
    pub truncated: Option<bool>,
}

impl CallRecord {
    pub fn duration_ms(&self) -> Option<i64> {
        self.finished_at.map(|end| end - self.started_at)
    }
}

/// One step inside a record: a session check, a lease, a policy write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    /// Unix milliseconds.
    pub at: i64,
    /// The scope the step ran under, which for authentication is the identity
    /// provider rather than the called tool.
    pub scope: String,
    pub name: String,
    pub detail: String,
}

/// What the call returned to the agent, kept apart from the record so listing
/// never has to open it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
}
