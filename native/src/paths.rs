//! Filesystem-path helpers for the transient screenshot files and app-name
//! sanitization. Recorded data now lives in the SQLite store (see [`crate::db`]);
//! the only files on disk are the PNGs waiting to be OCR'd, laid out flat as
//! `<data_dir>/<app>/<ts>-<display>.png` and deleted once contextualised.

use std::path::{Path, PathBuf};

/// Max byte length for a sanitized app-name folder (well under the 255-byte
/// filesystem limit, leaving room for nested paths).
const MAX_APP_NAME_BYTES: usize = 100;

/// `<data_dir>/<app>/` — the per-app folder holding that app's pending shots.
pub fn app_dir(data_dir: &Path, app_sanitized: &str) -> PathBuf {
    data_dir.join(app_sanitized)
}

/// `<data_dir>/<app>/<ts>-<display>.png` — one screenshot file. Deleted after
/// OCR; the `ts` and `display` in the name let a post-crash sweep recover the
/// metadata a leftover PNG needs.
pub fn shot_path(data_dir: &Path, app_sanitized: &str, ts: i64, display: u32) -> PathBuf {
    app_dir(data_dir, app_sanitized).join(format!("{ts}-{display}.png"))
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

    #[test]
    fn shot_path_is_flat_ts_display() {
        let p = shot_path(Path::new("/data"), "Chrome", 1751527334123, 2);
        assert!(p.ends_with("Chrome/1751527334123-2.png"));
    }
}
