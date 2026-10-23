//! A console table: a header in capitals, bold in a terminal, and every
//! column as wide as its widest cell, two spaces apart.

use super::style;

pub struct Table {
    header: Vec<&'static str>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// A table with these column names, written in capitals by the caller.
    pub fn new(header: &[&'static str]) -> Self {
        Self {
            header: header.to_vec(),
            rows: Vec::new(),
        }
    }

    pub fn row<I, S>(&mut self, cells: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows.push(cells.into_iter().map(Into::into).collect());
    }

    /// Print it, the header in bold when stdout is a terminal. A table with
    /// no rows prints nothing: an empty list stays empty.
    pub fn print(&self) {
        print!("{}", self.render(style::bold_on_stdout()));
    }

    pub fn render(&self, bold: bool) -> String {
        if self.rows.is_empty() {
            return String::new();
        }
        let widths = self.widths();
        let mut text = style::bold(&line(&self.header, &widths), bold);
        text.push('\n');
        for row in &self.rows {
            text.push_str(&line(row, &widths));
            text.push('\n');
        }
        text
    }

    fn widths(&self) -> Vec<usize> {
        let mut widths = self
            .header
            .iter()
            .map(|cell| cell.chars().count())
            .collect::<Vec<_>>();
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                let width = cell.chars().count();
                match widths.get_mut(index) {
                    Some(current) => *current = (*current).max(width),
                    None => widths.push(width),
                }
            }
        }
        widths
    }
}

/// One line, padded to the column widths; the last column is not padded.
fn line<S: AsRef<str>>(cells: &[S], widths: &[usize]) -> String {
    let mut text = String::new();
    for (index, cell) in cells.iter().enumerate() {
        let cell = cell.as_ref();
        if index > 0 {
            text.push_str("  ");
        }
        text.push_str(cell);
        if index + 1 < cells.len() {
            let pad = widths[index].saturating_sub(cell.chars().count());
            text.push_str(&" ".repeat(pad));
        }
    }
    text.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_line_up_under_their_header() {
        let mut table = Table::new(&["TARGET", "STATE", "REMAINING"]);
        table.row(["mdb-fsw-dev", "active", "306min"]);
        table.row(["prd", "inactive", "-"]);
        assert_eq!(
            table.render(false),
            "TARGET       STATE     REMAINING\n\
             mdb-fsw-dev  active    306min\n\
             prd          inactive  -\n"
        );
    }

    #[test]
    fn the_header_is_bold_only_when_asked() {
        let mut table = Table::new(&["NAME"]);
        table.row(["aws"]);
        assert_eq!(table.render(true), "\x1b[1mNAME\x1b[22m\naws\n");
        assert!(!table.render(false).contains('\x1b'));
    }

    #[test]
    fn an_empty_table_prints_nothing() {
        assert_eq!(Table::new(&["NAME"]).render(true), "");
    }

    #[test]
    fn an_empty_last_cell_leaves_no_trailing_space() {
        let mut table = Table::new(&["", "THEME", "NOTE"]);
        table.row(["*", "dark", "default"]);
        table.row([" ", "nord", ""]);
        assert_eq!(
            table.render(false),
            "   THEME  NOTE\n*  dark   default\n   nord\n"
        );
    }

    /// Accented names must not push the next column: width is in characters.
    #[test]
    fn width_counts_characters_not_bytes() {
        let mut table = Table::new(&["NAME", "COLOR"]);
        table.row(["grafite", "#2b2b2b"]);
        table.row(["âmbar", "#ffbf00"]);
        assert_eq!(
            table.render(false),
            "NAME     COLOR\ngrafite  #2b2b2b\nâmbar    #ffbf00\n"
        );
    }
}
