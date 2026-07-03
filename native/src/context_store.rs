//! OCR context store: one `contexts.jsonl` per `<date>/<app>/` holding the
//! text (with normalised on-screen coordinates) recognised from each deleted
//! screenshot, plus the paginated time-range query and the per-app usage
//! aggregation that power the MCP server.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;
use crate::writer;

/// One recognised text line. Coordinates are normalised to `[0, 1]` relative
/// to the captured display — `(x, y)` is the box's top-left, `(w, h)` its size
/// — so consumers can reason about where on screen the text sat (menu bar,
/// sidebar, content …) independent of resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrItem {
    pub text: String,
    /// Recognition confidence in `[0, 1]`.
    pub score: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One screenshot's OCR context. Matches the TypeScript `ContextRecordSchema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRecord {
    /// Epoch milliseconds the screenshot was taken.
    pub ts: i64,
    /// Sanitized, folder-safe app name.
    pub app: String,
    /// Original `localizedName` of the app.
    #[serde(rename = "appRaw")]
    pub app_raw: String,
    /// Display index the shot came from (0 = main display).
    pub display: u32,
    pub items: Vec<OcrItem>,
}

/// Per-app usage aggregate over a time range (keystroke records + contexts).
#[derive(Debug, Clone, Serialize)]
pub struct AppUsage {
    pub app: String,
    pub app_raw: String,
    pub first_ts: i64,
    pub last_ts: i64,
    pub text_records: u32,
    pub context_records: u32,
}

/// A page of context records: the range's total match count plus one slice.
pub struct ContextPage {
    pub total: u32,
    pub records: Vec<ContextRecord>,
}

/// Append one context record to `<data_dir>/<date>/<app>/contexts.jsonl`.
pub fn append_context(data_dir: &Path, record: &ContextRecord) -> Result<()> {
    let date = paths::local_date_for_ms(record.ts);
    let file = paths::contexts_file(data_dir, &date, &record.app);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let mut line = serde_json::to_string(record).context("serializing context record")?;
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

/// Scan the `<date>` folders spanning `[start_ms, end_ms]`, keep contexts whose
/// `ts` falls in range (and whose app matches `app`, when given — matched
/// case-insensitively against both the sanitized and raw names), sort them
/// ascending by `ts`, and return the `[offset, offset + limit)` slice plus the
/// total match count so callers can paginate.
pub fn query_contexts(
    data_dir: &Path,
    start_ms: i64,
    end_ms: i64,
    app: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<ContextPage> {
    let wanted = app.map(str::to_lowercase);
    let mut matches: Vec<ContextRecord> = Vec::new();

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
            let file = entry.path().join("contexts.jsonl");
            let f = match fs::File::open(&file) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for line in BufReader::new(f).lines().map_while(std::result::Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(record) = serde_json::from_str::<ContextRecord>(&line) else {
                    continue; // skip corrupt line
                };
                if record.ts > end_ms {
                    // Contexts are append-only in ts order; nothing later matches.
                    break;
                }
                if record.ts < start_ms {
                    continue;
                }
                if let Some(w) = &wanted {
                    if record.app.to_lowercase() != *w && record.app_raw.to_lowercase() != *w {
                        continue;
                    }
                }
                matches.push(record);
            }
        }
    }

    matches.sort_by_key(|r| r.ts);
    let total = matches.len() as u32;
    let records: Vec<ContextRecord> = matches.into_iter().skip(offset).take(limit).collect();
    Ok(ContextPage { total, records })
}

/// Aggregate per-app usage over `[start_ms, end_ms]` from both stores
/// (keystroke `records.jsonl` and OCR `contexts.jsonl`), sorted by most
/// recent activity first.
pub fn query_apps(data_dir: &Path, start_ms: i64, end_ms: i64) -> Result<Vec<AppUsage>> {
    use std::collections::HashMap;
    let mut by_app: HashMap<String, AppUsage> = HashMap::new();

    let mut touch = |app: &str, app_raw: &str, ts: i64, is_context: bool| {
        let usage = by_app.entry(app.to_string()).or_insert_with(|| AppUsage {
            app: app.to_string(),
            app_raw: app_raw.to_string(),
            first_ts: ts,
            last_ts: ts,
            text_records: 0,
            context_records: 0,
        });
        usage.first_ts = usage.first_ts.min(ts);
        usage.last_ts = usage.last_ts.max(ts);
        if is_context {
            usage.context_records += 1;
        } else {
            usage.text_records += 1;
        }
        // Prefer a non-empty raw name over the sanitized fallback.
        if usage.app_raw.is_empty() && !app_raw.is_empty() {
            usage.app_raw = app_raw.to_string();
        }
    };

    for record in writer::query(data_dir, start_ms, end_ms)? {
        touch(&record.app, &record.app_raw, record.ts, false);
    }
    // Contexts: reuse the range scan without pagination.
    let contexts = query_contexts(data_dir, start_ms, end_ms, None, 0, usize::MAX)?;
    for record in contexts.records {
        touch(&record.app, &record.app_raw, record.ts, true);
    }

    let mut out: Vec<AppUsage> = by_app.into_values().collect();
    out.sort_by_key(|u| std::cmp::Reverse(u.last_ts));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ts: i64, app: &str, texts: &[&str]) -> ContextRecord {
        ContextRecord {
            ts,
            app: app.to_string(),
            app_raw: app.to_string(),
            display: 0,
            items: texts
                .iter()
                .map(|t| OcrItem {
                    text: t.to_string(),
                    score: 0.9,
                    x: 0.1,
                    y: 0.2,
                    w: 0.3,
                    h: 0.02,
                })
                .collect(),
        }
    }

    #[test]
    fn append_query_paginate_and_aggregate() {
        let dir = std::env::temp_dir().join(format!("pi0-ctx-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let now = crate::paths::now_ms();
        append_context(&dir, &ctx(now, "Chrome", &["hello", "world"])).unwrap();
        append_context(&dir, &ctx(now + 10, "Chrome", &["again"])).unwrap();
        append_context(&dir, &ctx(now + 20, "Lark", &["message"])).unwrap();
        append_context(&dir, &ctx(now - 5 * 86_400_000, "Chrome", &["old"])).unwrap();

        // Full range: 3 recent records, paginated 2 + 1.
        let page = query_contexts(&dir, now - 3_600_000, now + 3_600_000, None, 0, 2).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.records.len(), 2);
        assert_eq!(page.records[0].items[0].text, "hello");
        let page2 = query_contexts(&dir, now - 3_600_000, now + 3_600_000, None, 2, 2).unwrap();
        assert_eq!(page2.total, 3);
        assert_eq!(page2.records.len(), 1);
        assert_eq!(page2.records[0].app, "Lark");

        // App filter is case-insensitive.
        let lark =
            query_contexts(&dir, now - 3_600_000, now + 3_600_000, Some("lark"), 0, 10).unwrap();
        assert_eq!(lark.total, 1);

        // Apps aggregation merges both stores (no text records here).
        let apps = query_apps(&dir, now - 3_600_000, now + 3_600_000).unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].app, "Lark", "most recent first");
        assert_eq!(apps[1].context_records, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
