use crate::config::ConfigPaths;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn log(paths: &ConfigPaths, provider: &str, event: &str, rule: &str, detail: &str) {
    let mut line = format!(
        "{} | {} | {} | {}",
        now_epoch(),
        provider,
        event,
        sanitize(rule)
    );
    if !detail.is_empty() {
        line.push_str(" | ");
        line.push_str(&sanitize(detail));
    }
    line.push('\n');
    let _ = paths.ensure();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log())
    {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sanitize(value: &str) -> String {
    value.replace(['\r', '\n', '|'], "_")
}
