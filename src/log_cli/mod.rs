//! `torii log`: follow, search, inspect and export the audit log.
//!
//! - `tail [-n <count>] [--follow]`: the latest records, one line each;
//!   `--follow` keeps watching and prints each call as it arrives and ends;
//! - `search [filters] [-n <count>]`: the newest records that match;
//! - `show <id> [--output]`: everything about one record;
//! - `export [filters] [--format json|md] [--file <path>] [--with-output]`;
//! - `prune [days]`: drop what is older than `days`, or than the retention.
//!
//! These commands are the human's: they open the sealed log with this
//! machine's key and never run through MCP.

mod filter;
mod render;

use std::collections::BTreeSet;
use std::time::Duration;

use crate::audit::{self, Ledger};
use crate::config::{logging, settings, ConfigPaths};
use crate::error::{Error, Result};

pub fn run(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["tail", ..] => tail(paths, &args[1..]),
        ["search", ..] => search(paths, &args[1..]),
        ["show", id, rest @ ..] => show(paths, id, rest),
        ["export", ..] => export(paths, &args[1..]),
        ["prune"] => prune(paths, None),
        ["prune", days] => prune(paths, Some(days)),
        _ => Err(usage(&words)),
    }
}

fn tail(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let mut count = None;
    let mut follow = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-n" | "--lines" => {
                count = Some(parse_count(args.get(index + 1))?);
                index += 2;
            }
            "-f" | "--follow" => {
                follow = true;
                index += 1;
            }
            other => {
                return Err(Error::Usage(format!(
                    "unexpected {other:?}; tail takes -n <count> and --follow"
                )))
            }
        }
    }
    let count = match count {
        Some(count) => count,
        None => settings::load(&paths.settings())?.log_tail_lines as usize,
    };
    let ledger = open(paths)?;
    let mut entries = ledger.newest(count).map_err(audit_error)?;
    entries.reverse();
    let mut last = entries.last().map_or(0, |entry| entry.id);
    let mut running = BTreeSet::new();
    for entry in &entries {
        println!("{}", render::line(entry));
        if is_running(entry) {
            running.insert(entry.id);
        }
    }
    if !follow {
        return Ok(());
    }
    // Each call prints when it arrives and again when it ends, so a call
    // waiting for a human answer is visible while it waits.
    loop {
        std::thread::sleep(Duration::from_millis(700));
        for id in running.clone() {
            if let Some(entry) = ledger.call(id).map_err(audit_error)? {
                if !is_running(&entry) {
                    println!("{}", render::line(&entry));
                    running.remove(&id);
                }
            } else {
                running.remove(&id);
            }
        }
        for entry in ledger.after(last).map_err(audit_error)? {
            last = entry.id;
            println!("{}", render::line(&entry));
            if is_running(&entry) {
                running.insert(entry.id);
            }
        }
    }
}

fn search(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let (mut query, rest) = filter::parse(args)?;
    let count = match rest.as_slice() {
        [] => settings::load(&paths.settings())?.log_tail_lines as usize,
        [flag, value] if flag == "-n" || flag == "--lines" => parse_count(Some(value))?,
        _ => {
            return Err(Error::Usage(format!(
                "unexpected {:?}\n\n{}",
                rest.join(" "),
                crate::help::nearest(&["log", "search"])
            )))
        }
    };
    query.limit = Some(count);
    let mut found = open(paths)?.search(&query).map_err(audit_error)?;
    found.reverse();
    if found.is_empty() {
        eprintln!("No record matches.");
    }
    for entry in &found {
        println!("{}", render::line(entry));
    }
    Ok(())
}

fn show(paths: &ConfigPaths, id: &str, rest: &[&str]) -> Result<()> {
    let with_output = match rest {
        [] => false,
        ["--output"] => true,
        _ => return Err(usage(&["show"])),
    };
    let id = parse_id(id)?;
    let ledger = open(paths)?;
    let entry = ledger
        .call(id)
        .map_err(audit_error)?
        .ok_or_else(|| Error::Usage(format!("there is no record #{id}")))?;
    let events = ledger.events(id).map_err(audit_error)?;
    let output = ledger.output(id).map_err(audit_error)?;
    print!(
        "{}",
        render::detail(
            &entry,
            &events,
            output.as_ref().filter(|_| with_output),
            output.is_some()
        )
    );
    if with_output && output.is_none() {
        eprintln!("#{id} has no stored output: it did not run, or log-output was none.");
    }
    Ok(())
}

fn export(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let (query, rest) = filter::parse(args)?;
    let mut format = Format::Json;
    let mut file = None;
    let mut with_output = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--format" => {
                format = match rest.get(index + 1).map(String::as_str) {
                    Some("json") => Format::Json,
                    Some("md" | "markdown") => Format::Markdown,
                    other => {
                        return Err(Error::Usage(format!(
                            "--format is json or md, not {other:?}"
                        )))
                    }
                };
                index += 2;
            }
            "--file" => {
                file = Some(
                    rest.get(index + 1)
                        .ok_or_else(|| Error::Usage("--file needs a path".into()))?
                        .clone(),
                );
                index += 2;
            }
            "--with-output" => {
                with_output = true;
                index += 1;
            }
            other => {
                return Err(Error::Usage(format!(
                    "unexpected {other:?}\n\n{}",
                    crate::help::nearest(&["log", "export"])
                )))
            }
        }
    }
    let ledger = open(paths)?;
    let mut entries = ledger.search(&query).map_err(audit_error)?;
    entries.reverse();
    let mut details = Vec::with_capacity(entries.len());
    for entry in &entries {
        let events = ledger.events(entry.id).map_err(audit_error)?;
        let output = if with_output {
            ledger.output(entry.id).map_err(audit_error)?
        } else {
            None
        };
        details.push((events, output));
    }
    let exported = entries
        .iter()
        .zip(&details)
        .map(|(entry, (events, output))| render::Exported {
            id: entry.id,
            record: &entry.record,
            events,
            output: output.as_ref(),
        })
        .collect::<Vec<_>>();
    let text = match format {
        Format::Json => serde_json::to_string_pretty(&exported)
            .map_err(|error| Error::Usage(error.to_string()))?,
        Format::Markdown => render::markdown(&exported),
    };
    match file {
        Some(file) => {
            std::fs::write(&file, text).map_err(|source| Error::Write {
                path: file.clone().into(),
                source,
            })?;
            eprintln!(
                "Exported {} record(s) to {file}. The export is not encrypted: keep it somewhere safe.",
                exported.len()
            );
        }
        None => {
            println!("{text}");
            eprintln!("The export is not encrypted: keep it somewhere safe.");
        }
    }
    Ok(())
}

enum Format {
    Json,
    Markdown,
}

fn prune(paths: &ConfigPaths, days: Option<&str>) -> Result<()> {
    let days = match days {
        Some(days) => logging::parse_count(days, &logging::RETENTION_DAYS).ok_or_else(|| {
            Error::Usage(format!(
                "prune keeps the last {}-{} days, not {days:?}",
                logging::RETENTION_DAYS.start(),
                logging::RETENTION_DAYS.end()
            ))
        })?,
        None => settings::load(&paths.settings())?.log_retention_days,
    };
    let removed = audit::prune(paths, days).map_err(audit_error)?;
    eprintln!("Removed {removed} record(s) older than {days} day(s).");
    Ok(())
}

fn open(paths: &ConfigPaths) -> Result<Ledger> {
    Ledger::open(paths).map_err(audit_error)
}

fn is_running(entry: &audit::Entry) -> bool {
    entry.record.outcome == audit::record::Outcome::Running
}

fn parse_count(value: Option<&String>) -> Result<usize> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .ok_or_else(|| Error::Usage("-n takes a positive number of records".into()))
}

fn parse_id(value: &str) -> Result<i64> {
    value
        .trim_start_matches('#')
        .parse::<i64>()
        .map_err(|_| Error::Usage(format!("{value:?} is not a record number such as 128")))
}

fn usage(words: &[&str]) -> Error {
    Error::Usage(format!(
        "unrecognized command line\n\n{}",
        crate::help::nearest(&[&["log"], words].concat())
    ))
}

fn audit_error(reason: String) -> Error {
    Error::Usage(format!("cannot read the audit log: {reason}"))
}
