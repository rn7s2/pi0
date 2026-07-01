//! JSONL keystroke-record store: one `records.jsonl` per `<date>/<app>/`, plus
//! the time-range query that powers the viewer.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;

/// One flushed chunk of keystrokes for a single app, tagged with the instant the
/// buffer started. Matches the TypeScript `TextRecordSchema` (zod) one-to-one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Epoch milliseconds (UTC instant) the buffered chunk began.
    pub ts: i64,
    /// Sanitized, folder-safe app name.
    pub app: String,
    /// Original `localizedName` of the app.
    #[serde(rename = "appRaw")]
    pub app_raw: String,
    /// The captured keystroke text (with the reference's modifier wrapping).
    pub text: String,
}

/// Append one record as a JSON line to `<data_dir>/<date>/<app>/records.jsonl`,
/// where `<date>` is the local calendar day of `record.ts`.
pub fn append_record(data_dir: &Path, record: &Record) -> Result<()> {
    let date = paths::local_date_for_ms(record.ts);
    let file = paths::records_file(data_dir, &date, &record.app);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let mut line = serde_json::to_string(record).context("serializing record")?;
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .with_context(|| format!("opening {}", file.display()))?;
    f.write_all(line.as_bytes())
        .with_context(|| format!("writing to {}", file.display()))?;
    Ok(())
}

/// Scan the `<date>` folders spanning `[start_ms, end_ms]`, read every app's
/// `records.jsonl`, keep records whose `ts` falls in range, and return them
/// sorted ascending by `ts`. Corrupt lines are skipped, not fatal.
pub fn query(data_dir: &Path, start_ms: i64, end_ms: i64) -> Result<Vec<Record>> {
    let mut out: Vec<Record> = Vec::new();

    for date in paths::dates_in_range(start_ms, end_ms) {
        let date_dir = data_dir.join(&date);
        let entries = match fs::read_dir(&date_dir) {
            Ok(e) => e,
            Err(_) => continue, // missing day folder — nothing recorded that day
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let file = entry.path().join("records.jsonl");
            let f = match fs::File::open(&file) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for line in BufReader::new(f).lines().map_while(std::result::Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(record) = serde_json::from_str::<Record>(&line) else {
                    continue; // skip corrupt line
                };
                if record.ts > end_ms {
                    // Records are append-only in ts order; nothing later matches.
                    break;
                }
                if record.ts >= start_ms {
                    out.push(record);
                }
            }
        }
    }

    out.sort_by_key(|r| r.ts);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: i64, app: &str, text: &str) -> Record {
        Record {
            ts,
            app: app.to_string(),
            app_raw: app.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn append_then_query_round_trip() {
        let dir = std::env::temp_dir().join(format!("pi0-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Three records "now" across two apps, plus one far in the past.
        let now = crate::paths::now_ms();
        append_record(&dir, &rec(now, "Chrome", "hello")).unwrap();
        append_record(&dir, &rec(now + 10, "Chrome", " world")).unwrap();
        append_record(&dir, &rec(now + 20, "Terminal", "ls")).unwrap();
        append_record(&dir, &rec(now - 5 * 86_400_000, "Chrome", "old")).unwrap();

        // Query the last hour → the three recent records, sorted by ts.
        let results = query(&dir, now - 3_600_000, now + 3_600_000).unwrap();
        assert_eq!(results.len(), 3, "expected 3 recent records");
        assert_eq!(results[0].text, "hello");
        assert_eq!(results[1].text, " world");
        assert_eq!(results[2].app, "Terminal");

        // A window excluding everything returns nothing.
        assert!(query(&dir, now + 10_000_000, now + 20_000_000)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
