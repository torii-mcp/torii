//! How record numbers read and how a typed one is found.
//!
//! In `tail` and `search` a number is right-aligned in a column at least
//! [`MIN_WIDTH`] digits wide, so every line keeps its columns, and its last
//! digits are bold: the fewest a human must type for `torii log show`. Numbers
//! are sequential and never reused, so the last `k` digits name a record
//! without ambiguity whenever the kept numbers span less than `10^k` — one `k`
//! for every line. The full number always works too, and is what exports and
//! tickets carry.

use crate::audit::Ledger;
use crate::console::style;
use crate::error::{Error, Result};

/// Seven digits hold ten million records; past that the column just widens.
const MIN_WIDTH: usize = 7;

/// How many candidates `show` lists when a suffix names several records.
const MAX_CANDIDATES: usize = 10;

pub(super) struct Ids {
    lowest: i64,
    highest: i64,
    bold: bool,
}

impl Ids {
    /// Numbering for the records `range` spans, in bold only when `bold`.
    pub(super) fn new(range: Option<(i64, i64)>, bold: bool) -> Self {
        let (lowest, highest) = range.unwrap_or((1, 1));
        Self {
            lowest,
            highest,
            bold,
        }
    }

    /// A record that arrived while following: it may widen the column or the
    /// suffix. Never narrows either, so lines already printed stay aligned.
    pub(super) fn include(&mut self, id: i64) {
        self.highest = self.highest.max(id);
        self.lowest = self.lowest.min(id);
    }

    /// `     #16` or `#1015312`, with the typable suffix in bold.
    pub(super) fn render(&self, id: i64) -> String {
        let digits = id.to_string();
        let pad = " ".repeat(self.digits().saturating_sub(digits.len()));
        let split = digits.len().saturating_sub(self.suffix());
        let (head, tail) = digits.split_at(split);
        format!("{pad}#{head}{}", style::bold(tail, self.bold))
    }

    /// How many characters a rendered number takes on screen, `#` included.
    pub(super) fn width(&self) -> usize {
        self.digits() + 1
    }

    /// Whether this listing goes to a terminal that shows bold.
    pub(super) fn bold(&self) -> bool {
        self.bold
    }

    fn digits(&self) -> usize {
        MIN_WIDTH.max(digit_count(self.highest))
    }

    /// How many trailing digits tell every kept record apart.
    fn suffix(&self) -> usize {
        digit_count(self.highest.saturating_sub(self.lowest))
    }
}

/// Which record `typed` names: the record with that exact number, else the
/// one whose number ends with those digits. A typed number with a leading zero
/// is only ever a suffix.
pub(super) fn resolve(ledger: &Ledger, typed: &str) -> Result<i64> {
    let digits = typed.trim_start_matches('#');
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Usage(format!(
            "{typed:?} is not a record number such as 128"
        )));
    }
    if !digits.starts_with('0') {
        if let Ok(id) = digits.parse::<i64>() {
            if ledger.call(id).map_err(super::audit_error)?.is_some() {
                return Ok(id);
            }
        }
    }
    let found = ledger
        .ending_with(digits, MAX_CANDIDATES + 1)
        .map_err(super::audit_error)?;
    match found.as_slice() {
        [] => Err(Error::Usage(format!("there is no record #{digits}"))),
        [id] => Ok(*id),
        several => {
            let shown = several
                .iter()
                .take(MAX_CANDIDATES)
                .map(|id| format!("#{id}"))
                .collect::<Vec<_>>()
                .join(", ");
            let more = if several.len() > MAX_CANDIDATES {
                ", …"
            } else {
                ""
            };
            Err(Error::Usage(format!(
                "{digits} ends several records ({shown}{more}); type more digits"
            )))
        }
    }
}

fn digit_count(value: i64) -> usize {
    value.max(1).to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_line_up_on_the_right_in_seven_columns() {
        let ids = Ids::new(Some((1, 120)), false);
        assert_eq!(ids.render(9), "      #9");
        assert_eq!(ids.render(120), "    #120");
    }

    #[test]
    fn the_column_widens_past_seven_digits() {
        let ids = Ids::new(Some((10_000_000, 12_345_678)), false);
        assert_eq!(ids.render(12_345_678), "#12345678");
    }

    /// After a prune the kept numbers span less than the numbers themselves:
    /// the bold suffix is how few digits are enough.
    #[test]
    fn the_bold_suffix_is_what_tells_kept_records_apart() {
        let ids = Ids::new(Some((1_000_500, 1_015_312)), true);
        assert_eq!(
            ids.render(1_015_312),
            format!("#10{}", style::bold("15312", true))
        );
        let young = Ids::new(Some((1, 16)), true);
        assert_eq!(
            young.render(9),
            format!("      #{}", style::bold("9", true))
        );
        assert_eq!(
            young.render(16),
            format!("     #{}", style::bold("16", true))
        );
    }

    #[test]
    fn a_followed_record_can_only_widen_the_suffix() {
        let mut ids = Ids::new(Some((95, 99)), true);
        assert_eq!(ids.suffix(), 1);
        ids.include(110);
        assert_eq!(ids.suffix(), 2);
    }

    #[test]
    fn without_bold_there_is_no_escape_at_all() {
        let ids = Ids::new(Some((1, 5_000)), false);
        assert!(!ids.render(4_321).contains('\x1b'));
    }

    #[test]
    fn a_typed_number_resolves_exactly_then_by_suffix() {
        use crate::audit::record::{CallRecord, Kind, Outcome};
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = crate::audit::test_ledger(temp.path());
        for index in 0..125 {
            let record = CallRecord {
                kind: Kind::Call,
                started_at: index,
                finished_at: None,
                tool: "aws".into(),
                target: None,
                scope: "aws".into(),
                args: vec!["get".into()],
                description: None,
                outcome: Outcome::Running,
                source: None,
                rule: None,
                error: None,
                exit_code: None,
                stdout_bytes: None,
                stderr_bytes: None,
                truncated: None,
            };
            ledger.insert_call(&record).unwrap();
        }
        // The full number always wins, even where a longer one ends with it.
        assert_eq!(resolve(&ledger, "16").unwrap(), 16);
        assert_eq!(resolve(&ledger, "#16").unwrap(), 16);

        // After a prune the low numbers are gone and the last digits name
        // the record that is left.
        ledger.prune(100).unwrap();
        assert_eq!(resolve(&ledger, "16").unwrap(), 116);
        assert_eq!(resolve(&ledger, "#25").unwrap(), 125);
        let several = resolve(&ledger, "5").unwrap_err().to_string();
        assert!(
            several.contains("#125, #115, #105"),
            "candidates are listed newest first: {several}"
        );
        assert!(resolve(&ledger, "016").is_err(), "116 ends with 116");
        assert!(resolve(&ledger, "9999").is_err());
        assert!(resolve(&ledger, "abc").is_err());
    }
}
