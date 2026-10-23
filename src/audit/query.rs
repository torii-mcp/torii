//! Which records a search keeps.
//!
//! The indexed fields (provider, target, outcome, kind, time) become a `WHERE`
//! clause; the free text is matched in Rust, after SQLite has narrowed the
//! rows, because SQLite only folds the case of ASCII letters and the log is
//! written in Portuguese as often as in English.

use rusqlite::types::Value;

use super::record::{CallRecord, Kind, Outcome};

#[derive(Debug, Default, Clone)]
pub struct Query {
    pub tool: Option<String>,
    pub target: Option<String>,
    /// Any of these outcomes; empty means all.
    pub outcomes: Vec<Outcome>,
    /// Only agent calls, or only human control-plane actions.
    pub kind: Option<Kind>,
    /// Unix milliseconds, inclusive.
    pub since: Option<i64>,
    pub until: Option<i64>,
    /// Case-insensitive, over the call, its reason, rule, error and scope.
    pub text: Option<String>,
    /// The newest this many; `None` for every match.
    pub limit: Option<usize>,
}

impl Query {
    /// The `WHERE` clause of the indexed filters, with its parameters.
    pub(super) fn sql(&self) -> (String, Vec<Value>) {
        let mut clauses = Vec::new();
        let mut values = Vec::new();
        if let Some(tool) = &self.tool {
            clauses.push("tool = ?".to_string());
            values.push(Value::Text(tool.clone()));
        }
        if let Some(target) = &self.target {
            clauses.push("target = ?".to_string());
            values.push(Value::Text(target.clone()));
        }
        if !self.outcomes.is_empty() {
            clauses.push(format!(
                "outcome IN ({})",
                vec!["?"; self.outcomes.len()].join(", ")
            ));
            values.extend(
                self.outcomes
                    .iter()
                    .map(|outcome| Value::Text(outcome.name().into())),
            );
        }
        if let Some(kind) = self.kind {
            clauses.push("kind = ?".to_string());
            values.push(Value::Text(kind.name().into()));
        }
        if let Some(since) = self.since {
            clauses.push("started_at >= ?".to_string());
            values.push(Value::Integer(since));
        }
        if let Some(until) = self.until {
            clauses.push("started_at <= ?".to_string());
            values.push(Value::Integer(until));
        }
        let clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        (clause, values)
    }

    /// Whether a record the indexed filters kept also holds the text.
    pub(super) fn matches_text(&self, record: &CallRecord) -> bool {
        let Some(text) = &self.text else {
            return true;
        };
        let needle = text.to_lowercase();
        [
            record.scope.as_str(),
            &record.args.join(" "),
            record.description.as_deref().unwrap_or(""),
            record.source.as_deref().unwrap_or(""),
            record.rule.as_deref().unwrap_or(""),
            record.error.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|field| field.to_lowercase().contains(&needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_filter_is_no_clause() {
        let (clause, values) = Query::default().sql();
        assert!(clause.is_empty());
        assert!(values.is_empty());
    }

    #[test]
    fn indexed_filters_combine_with_and_outcomes_with_or() {
        let query = Query {
            tool: Some("aws".into()),
            outcomes: vec![Outcome::Denied, Outcome::Error],
            since: Some(10),
            ..Query::default()
        };
        let (clause, values) = query.sql();
        assert_eq!(
            clause,
            "WHERE tool = ? AND outcome IN (?, ?) AND started_at >= ?"
        );
        assert_eq!(values.len(), 4);
    }
}
