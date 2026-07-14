pub mod grants;
pub mod rules;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionResult {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyDecision {
    pub result: DecisionResult,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}
