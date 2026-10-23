//! Emphasis on the console: bold, and only where a terminal will show it.

use std::io::IsTerminal;

const BOLD: &str = "\x1b[1m";
const NOT_BOLD: &str = "\x1b[22m";

/// `text` in bold when `on`, untouched otherwise.
pub fn bold(text: &str, on: bool) -> String {
    if on {
        format!("{BOLD}{text}{NOT_BOLD}")
    } else {
        text.to_owned()
    }
}

/// Bold only reaches a terminal that understands it: never a pipe or a file,
/// never with `NO_COLOR` or `TERM=dumb`.
pub fn bold_on_stdout() -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return false;
    }
    if std::env::var_os("TERM").is_some_and(|value| value == "dumb") {
        return false;
    }
    std::io::stdout().is_terminal() && console_takes_escapes()
}

/// The Windows console shows escape sequences as text unless asked not to.
#[cfg(windows)]
fn console_takes_escapes() -> bool {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_OUTPUT_HANDLE,
    };
    // SAFETY: plain Win32 calls on this process's own stdout handle; `mode`
    // outlives the call that writes it.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        GetConsoleMode(handle, &mut mode) != 0
            && (mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0
                || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0)
    }
}

#[cfg(not(windows))]
fn console_takes_escapes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_wraps_only_when_on() {
        assert_eq!(bold("ID", true), "\x1b[1mID\x1b[22m");
        assert_eq!(bold("ID", false), "ID");
    }
}
