//! How audit records read: one line for `tail` and `search`, the full story
//! for `show`, Markdown and JSON for `export`.

use chrono::{Local, TimeZone};
use serde::Serialize;

use crate::audit::record::{CallRecord, EventRecord, Kind, Outcome, Output};
use crate::audit::Entry;

/// How much of the call a summary line shows.
const CALL_CHARS: usize = 90;

/// `2026-10-23 14:32:07  #128  aws/prd       ALLOW  temporary grant     exit 0   1.2s  s3api list-objects-v2 …`
pub(super) fn line(entry: &Entry) -> String {
    let record = &entry.record;
    let (verdict, how, what) = match record.kind {
        Kind::Call => (
            verdict(record.outcome),
            how(record),
            shorten(&record.args.join(" "), CALL_CHARS),
        ),
        Kind::Control => (
            "HUMAN",
            record.source.clone().unwrap_or_default(),
            record.rule.clone().unwrap_or_default(),
        ),
    };
    let result = match (record.exit_code, record.duration_ms()) {
        (Some(code), Some(ms)) => format!("exit {code:<3} {}", duration(ms)),
        (None, Some(ms)) if record.kind == Kind::Call => format!("{:<8} {}", "", duration(ms)),
        _ => String::new(),
    };
    format!(
        "{}  #{:<5} {:<20} {:<5}  {:<26} {:<16} {what}",
        time(record.started_at),
        entry.id,
        shorten(&record.scope, 20),
        verdict,
        shorten(&how, 26),
        result,
    )
    .trim_end()
    .to_owned()
}

/// Everything about one record, for `show`.
pub(super) fn detail(
    entry: &Entry,
    events: &[EventRecord],
    output: Option<&Output>,
    has_output: bool,
) -> String {
    let record = &entry.record;
    let mut text = String::new();
    let mut field = |name: &str, value: &str| {
        if !value.is_empty() {
            text.push_str(&format!("{name:<12}{value}\n"));
        }
    };
    let ended = match record.finished_at {
        Some(end) => format!(" → {} ({})", time(end), duration(end - record.started_at)),
        None => " (not finished)".into(),
    };
    field(
        "record",
        &format!("#{}  {}{ended}", entry.id, time(record.started_at)),
    );
    field(
        "kind",
        match record.kind {
            Kind::Call => "agent call",
            Kind::Control => "human command",
        },
    );
    field("provider", &record.tool);
    field("target", record.target.as_deref().unwrap_or(""));
    if record.kind == Kind::Call {
        field("call", &quoted(&record.args));
        field("reason", record.description.as_deref().unwrap_or(""));
        field(
            "verdict",
            &format!("{} · {}", verdict(record.outcome), how(record)),
        );
        field("rule", record.rule.as_deref().unwrap_or(""));
    } else {
        field("action", record.source.as_deref().unwrap_or(""));
        field("detail", record.rule.as_deref().unwrap_or(""));
    }
    field("error", record.error.as_deref().unwrap_or(""));
    if let Some(code) = record.exit_code {
        field(
            "result",
            &format!(
                "exit {code} · stdout {} · stderr {}{}",
                bytes(record.stdout_bytes.unwrap_or(0)),
                bytes(record.stderr_bytes.unwrap_or(0)),
                if record.truncated == Some(true) {
                    " · truncated"
                } else {
                    ""
                }
            ),
        );
    }
    if !events.is_empty() {
        text.push_str("events\n");
        for event in events {
            text.push_str(&format!(
                "  {}  {:<26} {}{}\n",
                clock(event.at),
                event.name,
                if event.scope == record.scope {
                    String::new()
                } else {
                    format!("[{}] ", event.scope)
                },
                event.detail
            ));
        }
    }
    match output {
        Some(output) => {
            text.push_str("stdout\n");
            text.push_str(&indent(&output.stdout));
            if !output.stderr.is_empty() {
                text.push_str("stderr\n");
                text.push_str(&indent(&output.stderr));
            }
        }
        None if has_output => text.push_str(&format!(
            "output      stored; `torii log show {} --output` prints it\n",
            entry.id
        )),
        None => {}
    }
    text
}

/// One record as exported to JSON.
#[derive(Serialize)]
pub(super) struct Exported<'a> {
    pub id: i64,
    pub record: &'a CallRecord,
    pub events: &'a [EventRecord],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<&'a Output>,
}

pub(super) fn markdown(exported: &[Exported<'_>]) -> String {
    let mut text = String::from("# Torii audit log\n\n");
    text.push_str(&format!(
        "Exported {} · {} record(s)\n\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        exported.len()
    ));
    for item in exported {
        let record = item.record;
        text.push_str(&format!(
            "## #{} · {} · {}\n\n",
            item.id,
            record.scope,
            match record.kind {
                Kind::Call => verdict(record.outcome),
                Kind::Control => "HUMAN",
            }
        ));
        text.push_str("| | |\n|---|---|\n");
        let mut row = |name: &str, value: String| {
            if !value.is_empty() {
                text.push_str(&format!("| {name} | {} |\n", cell(&value)));
            }
        };
        row("started", time(record.started_at));
        row("finished", record.finished_at.map(time).unwrap_or_default());
        row("provider", record.tool.clone());
        row("target", record.target.clone().unwrap_or_default());
        if record.kind == Kind::Call {
            row("call", format!("`{}`", quoted(&record.args)));
            row("reason", record.description.clone().unwrap_or_default());
            row(
                "verdict",
                format!("{} · {}", verdict(record.outcome), how(record)),
            );
            row("rule", record.rule.clone().unwrap_or_default());
        } else {
            row("action", record.source.clone().unwrap_or_default());
            row("detail", record.rule.clone().unwrap_or_default());
        }
        row("error", record.error.clone().unwrap_or_default());
        if let Some(code) = record.exit_code {
            row(
                "result",
                format!(
                    "exit {code} · stdout {} · stderr {}",
                    bytes(record.stdout_bytes.unwrap_or(0)),
                    bytes(record.stderr_bytes.unwrap_or(0))
                ),
            );
        }
        text.push('\n');
        if !item.events.is_empty() {
            text.push_str("Events:\n\n");
            for event in item.events {
                text.push_str(&format!(
                    "- `{}` **{}** {}\n",
                    clock(event.at),
                    event.name,
                    event.detail
                ));
            }
            text.push('\n');
        }
        if let Some(output) = item.output {
            text.push_str(&format!(
                "stdout:\n\n````text\n{}\n````\n\n",
                output.stdout.trim_end()
            ));
            if !output.stderr.is_empty() {
                text.push_str(&format!(
                    "stderr:\n\n````text\n{}\n````\n\n",
                    output.stderr.trim_end()
                ));
            }
        }
    }
    text
}

fn verdict(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Running => "RUN",
        Outcome::Allowed => "ALLOW",
        Outcome::Denied => "DENY",
        Outcome::Error => "ERROR",
        Outcome::Done => "DONE",
    }
}

/// How the verdict was reached, in words.
fn how(record: &CallRecord) -> String {
    if record.outcome == Outcome::Error {
        return record
            .error
            .as_deref()
            .map(|error| error.lines().next().unwrap_or(error).to_owned())
            .unwrap_or_default();
    }
    let Some(source) = record.source.as_deref() else {
        return match record.outcome {
            Outcome::Running => "waiting or running".into(),
            _ => String::new(),
        };
    };
    match source {
        "rules" => "accept rule",
        "grant" => "temporary grant",
        "human-once" => "approved once",
        "human-grant" => "approved for a while",
        "human-permanent-allow" => "approved, accept written",
        "human-permanent-deny" => "denied, deny written",
        "human-deny" => "denied in the window",
        "headless-deny" => "headless, no window",
        "explicit-deny" => "deny rule",
        "forbidden-arg" => "forbidden argument",
        "target-inactive" => "target not authorized",
        "target-access-lost" => "target lease lost",
        other => other,
    }
    .to_owned()
}

/// Local date and time.
fn time(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "?".into())
}

/// Local time of day with milliseconds, for the steps of one record.
fn clock(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|time| time.format("%H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "?".into())
}

fn duration(ms: i64) -> String {
    match ms {
        ..1_000 => format!("{ms}ms"),
        1_000..60_000 => format!("{:.1}s", ms as f64 / 1000.0),
        _ => format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000),
    }
}

fn bytes(count: u64) -> String {
    match count {
        ..1024 => format!("{count} B"),
        1024..1_048_576 => format!("{:.1} KiB", count as f64 / 1024.0),
        _ => format!("{:.1} MiB", count as f64 / 1_048_576.0),
    }
}

/// The argv as a human reads it: words with spaces or quotes are quoted.
fn quoted(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.is_empty() || arg.contains([' ', '"', '\'', '\t']) {
                serde_json::to_string(arg).unwrap_or_else(|_| arg.clone())
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn shorten(text: &str, max: usize) -> String {
    let flat = text.replace(['\n', '\r', '\t'], " ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut short = flat.chars().take(max - 1).collect::<String>();
        short.push('…');
        short
    }
}

fn indent(text: &str) -> String {
    let mut indented = text
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect::<String>();
    if indented.is_empty() {
        indented.push_str("  (empty)\n");
    }
    indented
}

/// A Markdown table cell: one line, pipes escaped.
fn cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> CallRecord {
        CallRecord {
            kind: Kind::Call,
            started_at: 1_700_000_000_000,
            finished_at: Some(1_700_000_001_200),
            tool: "aws".into(),
            target: Some("prd".into()),
            scope: "aws/prd".into(),
            args: ["s3api", "list-objects-v2", "--prefix", "a b"]
                .map(String::from)
                .to_vec(),
            description: Some("Conferir logs".into()),
            outcome: Outcome::Allowed,
            source: Some("grant".into()),
            rule: Some("prefix s3api list-objects-v2".into()),
            error: None,
            exit_code: Some(0),
            stdout_bytes: Some(2048),
            stderr_bytes: Some(0),
            truncated: Some(false),
        }
    }

    #[test]
    fn a_line_says_who_what_and_how() {
        let line = line(&Entry {
            id: 128,
            record: record(),
        });
        for part in [
            "#128",
            "aws/prd",
            "ALLOW",
            "temporary grant",
            "exit 0",
            "1.2s",
            "s3api list-objects-v2",
        ] {
            assert!(line.contains(part), "{part} missing from {line}");
        }
    }

    #[test]
    fn the_detail_quotes_the_call_and_lists_the_steps() {
        let event = EventRecord {
            at: 1_700_000_000_100,
            scope: "aws".into(),
            name: "session-ok".into(),
            detail: String::new(),
        };
        let text = detail(
            &Entry {
                id: 7,
                record: record(),
            },
            &[event],
            None,
            true,
        );
        assert!(text.contains(r#"--prefix "a b""#), "{text}");
        assert!(text.contains("Conferir logs"));
        assert!(text.contains("session-ok"));
        assert!(
            text.contains("[aws]"),
            "the identity provider scope shows: {text}"
        );
        assert!(text.contains("torii log show 7 --output"));
        assert!(text.contains("2.0 KiB"));
    }

    #[test]
    fn errors_say_what_failed() {
        let mut failed = record();
        failed.outcome = Outcome::Error;
        failed.error = Some("could not find the executable \"aws\" in PATH\nmore".into());
        let line = line(&Entry {
            id: 3,
            record: failed,
        });
        assert!(
            line.contains("ERROR") && line.contains("could not find"),
            "{line}"
        );
    }

    #[test]
    fn markdown_keeps_pipes_inside_their_cell() {
        let mut piped = record();
        piped.description = Some("a | b".into());
        let text = markdown(&[Exported {
            id: 1,
            record: &piped,
            events: &[],
            output: None,
        }]);
        assert!(text.contains(r"a \| b"), "{text}");
    }
}
