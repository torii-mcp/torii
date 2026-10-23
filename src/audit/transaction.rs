//! Recording one MCP call, or one control-plane action, as it happens.
//!
//! A [`Transaction`] begins before anything is decided, so even a call refused
//! for a malformed target leaves a record. Each step adds an event; closing
//! writes the verdict, how it was reached and what the execution returned.
//! Recording is best-effort: a failing audit log warns once on stderr and the
//! call goes on, exactly as it would without one.

use std::sync::Mutex;

use super::ledger::Ledger;
use super::record::{CallRecord, EventRecord, Kind, Outcome, Output};
use super::{now_ms, warn_once};
use crate::config::ConfigPaths;

/// What is known of a call when it begins.
pub struct Call<'a> {
    pub tool: &'a str,
    pub target: Option<&'a str>,
    pub args: &'a [String],
    pub description: Option<&'a str>,
    /// Whether to keep what the call returns (`log_output` in the settings).
    pub keep_output: bool,
}

/// How a call ended.
pub struct Closing<'a> {
    pub outcome: Outcome,
    pub source: Option<&'a str>,
    pub rule: Option<&'a str>,
    pub error: Option<String>,
    pub execution: Option<Execution<'a>>,
}

pub struct Execution<'a> {
    pub exit_code: i32,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub truncated: bool,
}

pub struct Transaction {
    /// `None` when the audit log could not be opened; every method is then a
    /// no-op.
    open: Option<Mutex<Open>>,
    keep_output: bool,
}

struct Open {
    ledger: Ledger,
    id: i64,
    record: CallRecord,
    /// How the record says the verdict was reached, when that differs from
    /// the name the MCP answer gives it.
    reworded_source: Option<String>,
}

impl Transaction {
    pub fn begin(paths: &ConfigPaths, call: Call<'_>) -> Self {
        let scope = match call.target {
            Some(target) => format!("{}/{target}", call.tool),
            None => call.tool.to_owned(),
        };
        let record = CallRecord {
            kind: Kind::Call,
            started_at: now_ms(),
            finished_at: None,
            tool: call.tool.to_owned(),
            target: call.target.map(str::to_owned),
            scope,
            args: call.args.to_vec(),
            description: call.description.map(str::to_owned),
            outcome: Outcome::Running,
            source: None,
            rule: None,
            error: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            truncated: None,
        };
        let open = Ledger::open(paths)
            .and_then(|ledger| {
                let id = ledger.insert_call(&record)?;
                Ok(Open {
                    ledger,
                    id,
                    record,
                    reworded_source: None,
                })
            })
            .map_err(|reason| warn_once(&reason))
            .ok();
        Self {
            open: open.map(Mutex::new),
            keep_output: call.keep_output,
        }
    }

    /// A step of the call, under the call's own scope.
    pub fn event(&self, name: &str, detail: &str) {
        self.with(|open| {
            let scope = open.record.scope.clone();
            add_event(open, &scope, name, detail)
        });
    }

    /// A step that runs under another scope, such as the identity provider
    /// that authenticates the call.
    pub fn event_in(&self, scope: &str, name: &str, detail: &str) {
        self.with(|open| add_event(open, scope, name, detail));
    }

    /// Record the verdict under `source` instead of the one the call returns.
    pub fn reword_source(&self, source: &str) {
        self.with(|open| {
            open.reworded_source = Some(source.to_owned());
            Ok(())
        });
    }

    pub fn close(&self, closing: Closing<'_>) {
        let keep_output = self.keep_output;
        self.with(|open| {
            let record = &mut open.record;
            record.finished_at = Some(now_ms());
            record.outcome = closing.outcome;
            record.source = open
                .reworded_source
                .take()
                .or_else(|| closing.source.map(str::to_owned));
            record.rule = closing.rule.map(str::to_owned);
            record.error = closing.error;
            if let Some(execution) = &closing.execution {
                record.exit_code = Some(execution.exit_code);
                record.stdout_bytes = Some(execution.stdout.len() as u64);
                record.stderr_bytes = Some(execution.stderr.len() as u64);
                record.truncated = Some(execution.truncated);
            }
            open.ledger.update_call(open.id, &open.record)?;
            match closing.execution {
                Some(execution) if keep_output => open.ledger.put_output(
                    open.id,
                    &Output {
                        stdout: execution.stdout.to_owned(),
                        stderr: execution.stderr.to_owned(),
                    },
                ),
                _ => Ok(()),
            }
        });
    }

    fn with(&self, write: impl FnOnce(&mut Open) -> Result<(), String>) {
        let Some(open) = &self.open else {
            return;
        };
        let mut open = open.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(reason) = write(&mut open) {
            warn_once(&reason);
        }
    }
}

fn add_event(open: &mut Open, scope: &str, name: &str, detail: &str) -> Result<(), String> {
    open.ledger.add_event(
        open.id,
        &EventRecord {
            at: now_ms(),
            scope: scope.to_owned(),
            name: name.to_owned(),
            detail: detail.to_owned(),
        },
    )
}

/// Where the steps of an authentication or lease go: into the record of the
/// call that needs them, or, from a human command, into a record of their own.
#[derive(Clone, Copy)]
pub enum Trail<'a> {
    Call(&'a Transaction),
    Control(&'a ConfigPaths),
}

impl Trail<'_> {
    pub fn event(self, scope: &str, name: &str, detail: &str) {
        match self {
            Trail::Call(transaction) => transaction.event_in(scope, name, detail),
            Trail::Control(paths) => record(paths, scope, name, detail),
        }
    }
}

/// A human control-plane action: one record holding one event.
pub fn record(paths: &ConfigPaths, scope: &str, name: &str, detail: &str) {
    let (tool, target) = match scope.split_once('/') {
        Some((tool, target)) => (tool, Some(target)),
        None => (scope, None),
    };
    let at = now_ms();
    let record = CallRecord {
        kind: Kind::Control,
        started_at: at,
        finished_at: Some(at),
        tool: tool.to_owned(),
        target: target.map(str::to_owned),
        scope: scope.to_owned(),
        args: Vec::new(),
        description: None,
        outcome: Outcome::Done,
        source: Some(name.to_owned()),
        rule: (!detail.is_empty()).then(|| detail.to_owned()),
        error: None,
        exit_code: None,
        stdout_bytes: None,
        stderr_bytes: None,
        truncated: None,
    };
    let written = Ledger::open(paths).and_then(|ledger| {
        let id = ledger.insert_call(&record)?;
        ledger.add_event(
            id,
            &EventRecord {
                at,
                scope: scope.to_owned(),
                name: name.to_owned(),
                detail: detail.to_owned(),
            },
        )
    });
    if let Err(reason) = written {
        warn_once(&reason);
    }
}
