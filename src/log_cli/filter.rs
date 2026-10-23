//! The filter flags of `torii log search` and `export`, read into a query the
//! audit database runs.

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};

use crate::audit::record::{Kind, Outcome};
use crate::audit::{now_ms, Query};
use crate::error::{Error, Result};

/// Parse the filter flags, returning the words that are not filters for the
/// command's own options.
pub(super) fn parse(args: &[String]) -> Result<(Query, Vec<String>)> {
    let mut query = Query::default();
    let mut rest = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = || {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| Error::Usage(format!("{flag} needs a value")))
        };
        match flag {
            "--provider" => query.tool = Some(value()?),
            "--target" => query.target = Some(value()?),
            "--text" => query.text = Some(value()?),
            "--since" => query.since = Some(when(&value()?, Edge::Start)?),
            "--until" => query.until = Some(when(&value()?, Edge::End)?),
            "--allowed" | "--denied" | "--errors" | "--running" => {
                query.outcomes.push(match flag {
                    "--allowed" => Outcome::Allowed,
                    "--denied" => Outcome::Denied,
                    "--errors" => Outcome::Error,
                    _ => Outcome::Running,
                });
                index += 1;
                continue;
            }
            "--human" | "--agent" => {
                query.kind = Some(if flag == "--human" {
                    Kind::Control
                } else {
                    Kind::Call
                });
                index += 1;
                continue;
            }
            _ => {
                rest.push(args[index].clone());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    Ok((query, rest))
}

#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
}

/// `30m`, `2h` and `7d` reach back from now; `2026-10-20` is a local day, its
/// start for `--since` and its end for `--until`; `2026-10-20T14:30` is a local
/// minute.
fn when(value: &str, edge: Edge) -> Result<i64> {
    let invalid = || {
        Error::Usage(format!(
            "{value:?} is not a time; use 30m, 2h, 7d, 2026-10-20 or 2026-10-20T14:30"
        ))
    };
    if let Some(amount) = value.strip_suffix(['m', 'h', 'd']) {
        let amount = amount.parse::<i64>().map_err(|_| invalid())?;
        let unit = match value.chars().last() {
            Some('m') => 60_000,
            Some('h') => 3_600_000,
            _ => 86_400_000,
        };
        return Ok(now_ms() - amount * unit);
    }
    let local = if let Ok(day) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        match edge {
            Edge::Start => day.and_hms_opt(0, 0, 0),
            Edge::End => day.and_hms_milli_opt(23, 59, 59, 999),
        }
        .ok_or_else(invalid)?
    } else {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").map_err(|_| invalid())?
    };
    Local
        .from_local_datetime(&local)
        .earliest()
        .map(|time| time.timestamp_millis())
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(words: &[&str]) -> Query {
        let args = words
            .iter()
            .map(|word| word.to_string())
            .collect::<Vec<_>>();
        let (query, rest) = parse(&args).unwrap();
        assert!(rest.is_empty(), "{rest:?}");
        query
    }

    #[test]
    fn flags_fill_the_query() {
        let query = query(&[
            "--provider",
            "aws",
            "--target",
            "prd",
            "--denied",
            "--errors",
            "--agent",
            "--text",
            "s3 rm",
        ]);
        assert_eq!(query.tool.as_deref(), Some("aws"));
        assert_eq!(query.target.as_deref(), Some("prd"));
        assert_eq!(query.outcomes, [Outcome::Denied, Outcome::Error]);
        assert_eq!(query.kind, Some(Kind::Call));
        assert_eq!(query.text.as_deref(), Some("s3 rm"));
    }

    #[test]
    fn time_windows_reach_back_from_now() {
        let before = now_ms();
        let since = query(&["--since", "1h"]).since.unwrap();
        assert!((before - since - 3_600_000).abs() < 5_000);
        assert!(query(&["--since", "2026-10-20T14:30"]).since.is_some());
        assert!(parse(&["--since".into(), "ontem".into()]).is_err());
        assert!(parse(&["--since".into()]).is_err());
    }

    #[test]
    fn a_day_ends_at_its_last_millisecond() {
        let start = when("2026-10-20", Edge::Start).unwrap();
        let end = when("2026-10-20", Edge::End).unwrap();
        assert_eq!(end - start, 86_400_000 - 1);
    }

    #[test]
    fn unknown_words_are_left_to_the_command() {
        let args = ["--format", "md", "--denied"].map(String::from).to_vec();
        let (query, rest) = parse(&args).unwrap();
        assert_eq!(query.outcomes, [Outcome::Denied]);
        assert_eq!(rest, ["--format", "md"]);
    }
}
