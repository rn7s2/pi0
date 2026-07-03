//! Filesystem-path helpers: `DATA_DIR` layout, local dates, and app-name
//! sanitization so a frontmost-app name can safely become a folder name.

use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};

/// Max byte length for a sanitized app-name folder (well under the 255-byte
/// filesystem limit, leaving room for nested paths).
const MAX_APP_NAME_BYTES: usize = 100;

/// Local calendar date (`YYYY-MM-DD`) for a given epoch-millisecond instant.
pub fn local_date_for_ms(ts_ms: i64) -> String {
    let dt = Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(Local::now);
    dt.format("%Y-%m-%d").to_string()
}

/// Local calendar date (`YYYY-MM-DD`) for right now.
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Current time as epoch milliseconds.
pub fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

/// `<data_dir>/<date>/<app>/` — the per-app folder for a given day.
pub fn app_dir(data_dir: &Path, date: &str, app_sanitized: &str) -> PathBuf {
    data_dir.join(date).join(app_sanitized)
}

/// The keystroke record file for a given day + app.
pub fn records_file(data_dir: &Path, date: &str, app_sanitized: &str) -> PathBuf {
    app_dir(data_dir, date, app_sanitized).join("records.jsonl")
}

/// The screenshots folder for a given day + app.
pub fn shots_dir(data_dir: &Path, date: &str, app_sanitized: &str) -> PathBuf {
    app_dir(data_dir, date, app_sanitized).join("shots")
}

/// The OCR context file for a given day + app.
pub fn contexts_file(data_dir: &Path, date: &str, app_sanitized: &str) -> PathBuf {
    app_dir(data_dir, date, app_sanitized).join("contexts.jsonl")
}

/// The inclusive set of local dates spanned by `[start_ms, end_ms]`, as
/// `YYYY-MM-DD` strings. Used to pick which `<date>` folders a query scans.
pub fn dates_in_range(start_ms: i64, end_ms: i64) -> Vec<String> {
    if end_ms < start_ms {
        return Vec::new();
    }
    let start_day = Local
        .timestamp_millis_opt(start_ms)
        .single()
        .unwrap_or_else(Local::now)
        .date_naive();
    let end_day = Local
        .timestamp_millis_opt(end_ms)
        .single()
        .unwrap_or_else(Local::now)
        .date_naive();

    let mut out = Vec::new();
    let mut day = start_day;
    while day <= end_day {
        out.push(day.format("%Y-%m-%d").to_string());
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    out
}

/// Turn a frontmost-app name into a filesystem-safe folder name.
///
/// Replaces the macOS path separators (`/` and the legacy `:`), strips control
/// characters, collapses separator runs, trims surrounding whitespace/dots, and
/// falls back to `Unknown` for empty / reserved (`.`, `..`) results. The raw
/// name is preserved separately in each record's `appRaw` field.
pub fn sanitize_app_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_was_dash = false;
    for ch in raw.chars() {
        let mapped = match ch {
            '/' | ':' | '\\' => Some('-'),
            c if (c as u32) < 0x20 || c == '\u{7f}' => None, // drop control chars
            c => Some(c),
        };
        match mapped {
            Some('-') => {
                if !last_was_dash {
                    out.push('-');
                    last_was_dash = true;
                }
            }
            Some(c) => {
                out.push(c);
                last_was_dash = false;
            }
            None => {}
        }
    }

    let trimmed = out.trim().trim_matches('.').trim();
    let mut result = trimmed.to_string();

    // Enforce a byte cap on a UTF-8 boundary.
    if result.len() > MAX_APP_NAME_BYTES {
        let mut end = MAX_APP_NAME_BYTES;
        while end > 0 && !result.is_char_boundary(end) {
            end -= 1;
        }
        result.truncate(end);
        result = result.trim().trim_matches('.').trim().to_string();
    }

    if result.is_empty() || result == "." || result == ".." {
        "Unknown".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_separators_and_control_chars() {
        assert_eq!(sanitize_app_name("Google/Chrome"), "Google-Chrome");
        assert_eq!(sanitize_app_name("a:b"), "a-b");
        assert_eq!(sanitize_app_name("tab\there"), "tabhere");
        assert_eq!(sanitize_app_name("  spaced  "), "spaced");
        assert_eq!(sanitize_app_name(""), "Unknown");
        assert_eq!(sanitize_app_name("..."), "Unknown");
        assert_eq!(sanitize_app_name("a//b"), "a-b");
    }
}
