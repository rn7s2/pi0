//! The pi0 data store: a single password-protected SQLite database (SQLCipher,
//! WAL) at `<data_dir>/pi0.db`. Everything pi0 records — keystroke text, OCR
//! screen contexts, and the MCP access token — lives here, encrypted at rest
//! with a key derived from the user's password.
//!
//! The connection is process-global behind a `Mutex`: the HID thread (text
//! records), the OCR thread (contexts), and the libuv query workers all share
//! one authenticated connection. pi0's write volume is a handful of rows per
//! second, so serialising through one mutexed connection is simpler than a pool
//! and more than fast enough. The DB stays locked until [`open`] succeeds with
//! the correct password — every store/query call fails closed while locked.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, Row};

use crate::context_store::{
    AppUsage, ContextRecord, OcrItem, TimelineKind, TimelinePage, TimelineRecord,
};
use crate::writer::Record;

/// The open, authenticated database plus the password it was unlocked with (kept
/// so a password *change* can verify the current one without a second open).
struct Db {
    conn: Connection,
    password: String,
}

/// The process-global connection slot: `None` until unlocked.
fn slot() -> &'static Mutex<Option<Db>> {
    static SLOT: OnceLock<Mutex<Option<Db>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// `<data_dir>/pi0.db` — the single database file (WAL adds `-wal`/`-shm`).
pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("pi0.db")
}

/// Whether a database file already exists under `data_dir` (i.e. not first run).
pub fn exists(data_dir: &Path) -> bool {
    db_path(data_dir).exists()
}

/// Whether the database is currently unlocked (a connection is held).
pub fn is_open() -> bool {
    slot().lock().unwrap().is_some()
}

/// Open (or create, on first run) the encrypted database with `password`.
///
/// Returns `true` if the database was newly created. Fails with
/// `"incorrect password"` if an existing database can't be decrypted with the
/// given key. Re-opening while already unlocked replaces the held connection.
pub fn open(data_dir: &Path, password: &str) -> Result<bool> {
    let path = db_path(data_dir);
    let existed = path.exists();
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating data dir {}", data_dir.display()))?;

    let conn = Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
    // The key must be applied before any other access; SQLCipher derives the
    // encryption key from it via PBKDF2.
    conn.pragma_update(None, "key", password)
        .context("applying database key")?;
    // Verify the key: on an existing DB a wrong password makes the first read
    // fail (SQLITE_NOTADB); on a new/empty file this simply returns 0.
    let verified: rusqlite::Result<i64> =
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0));
    if verified.is_err() {
        bail!("incorrect password");
    }
    conn.pragma_update(None, "journal_mode", "WAL")
        .context("enabling WAL")?;
    migrate(&conn)?;

    *slot().lock().unwrap() = Some(Db {
        conn,
        password: password.to_string(),
    });
    Ok(!existed)
}

/// Change the database password (re-encrypts in place via SQLCipher `rekey`).
/// Verifies `current` against the password the DB was unlocked with first.
pub fn change_password(current: &str, new: &str) -> Result<()> {
    let mut guard = slot().lock().unwrap();
    let db = guard
        .as_mut()
        .ok_or_else(|| anyhow!("database is locked"))?;
    if db.password != current {
        bail!("incorrect current password");
    }
    db.conn
        .pragma_update(None, "rekey", new)
        .context("re-encrypting database")?;
    db.password = new.to_string();
    Ok(())
}

/// Return the MCP access token, minting and persisting a fresh 256-bit one on
/// first call. Stable across restarts and password changes (it lives in `meta`).
pub fn mcp_token() -> Result<String> {
    let guard = slot().lock().unwrap();
    let db = guard
        .as_ref()
        .ok_or_else(|| anyhow!("database is locked"))?;
    db.conn
        .execute(
            "INSERT OR IGNORE INTO meta(key, value) \
             VALUES ('mcp_token', lower(hex(randomblob(32))))",
            [],
        )
        .context("creating mcp token")?;
    db.conn
        .query_row("SELECT value FROM meta WHERE key = 'mcp_token'", [], |r| {
            r.get(0)
        })
        .context("reading mcp token")
        .map_err(Into::into)
}

/// Persist one keystroke record.
pub fn insert_text_record(rec: &Record) -> Result<()> {
    with_db(|db| {
        db.conn
            .execute(
                "INSERT INTO text_records(ts, local_time, tz_name, app, app_raw, text) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rec.ts,
                    rec.local_time,
                    rec.tz_name,
                    rec.app,
                    rec.app_raw,
                    rec.text
                ],
            )
            .context("inserting text record")?;
        Ok(())
    })
}

/// Persist one OCR screen context (its items are stored as a JSON array).
pub fn insert_context(rec: &ContextRecord) -> Result<()> {
    let items = serde_json::to_string(&rec.items).context("serializing OCR items")?;
    with_db(|db| {
        db.conn
            .execute(
                "INSERT INTO contexts(ts, local_time, tz_name, app, app_raw, display, items) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    rec.ts,
                    rec.local_time,
                    rec.tz_name,
                    rec.app,
                    rec.app_raw,
                    rec.display,
                    items
                ],
            )
            .context("inserting context")?;
        Ok(())
    })
}

/// All keystroke records within `[start_ms, end_ms]`, ascending by `ts`.
pub fn query_text(start_ms: i64, end_ms: i64) -> Result<Vec<Record>> {
    with_db(|db| {
        let mut stmt = db.conn.prepare(
            "SELECT ts, local_time, tz_name, app, app_raw, text FROM text_records \
             WHERE ts BETWEEN ?1 AND ?2 ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![start_ms, end_ms], |r| {
            Ok(Record {
                ts: r.get(0)?,
                local_time: r.get(1)?,
                tz_name: r.get(2)?,
                app: r.get(3)?,
                app_raw: r.get(4)?,
                text: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// One page of the merged activity timeline within `[start_ms, end_ms]` — OCR
/// contexts and keystroke records interleaved in `ts` order — optionally filtered
/// to one app (matched case-insensitively against sanitized *or* raw name), plus
/// the combined total match count for pagination.
///
/// Both stores are read in a single `UNION ALL` with a shared projection so the
/// `ORDER BY ts` / `LIMIT` / `OFFSET` apply to the merged stream. Each subquery
/// pads the columns it lacks with `NULL` (`text` for contexts, `display`/`items`
/// for keystrokes); `row_to_timeline` reads only the columns its `kind` populates.
pub fn query_timeline(
    start_ms: i64,
    end_ms: i64,
    app: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<TimelinePage> {
    with_db(|db| {
        let (total, records) = match app {
            Some(app) => {
                let app = app.to_lowercase();
                let total: u32 = db.conn.query_row(
                    "SELECT \
                       (SELECT COUNT(*) FROM contexts \
                          WHERE ts BETWEEN ?1 AND ?2 AND (lower(app) = ?3 OR lower(app_raw) = ?3)) \
                     + (SELECT COUNT(*) FROM text_records \
                          WHERE ts BETWEEN ?1 AND ?2 AND (lower(app) = ?3 OR lower(app_raw) = ?3))",
                    params![start_ms, end_ms, app],
                    |r| r.get(0),
                )?;
                let mut stmt = db.conn.prepare(
                    "SELECT ts, local_time, tz_name, app, app_raw, 'ocr' AS kind, display, items, NULL AS text \
                       FROM contexts \
                       WHERE ts BETWEEN ?1 AND ?2 AND (lower(app) = ?3 OR lower(app_raw) = ?3) \
                     UNION ALL \
                     SELECT ts, local_time, tz_name, app, app_raw, 'keys' AS kind, NULL AS display, NULL AS items, text \
                       FROM text_records \
                       WHERE ts BETWEEN ?1 AND ?2 AND (lower(app) = ?3 OR lower(app_raw) = ?3) \
                     ORDER BY ts LIMIT ?4 OFFSET ?5",
                )?;
                let rows = stmt.query_map(
                    params![start_ms, end_ms, app, limit as i64, offset as i64],
                    row_to_timeline,
                )?;
                (total, rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
            None => {
                let total: u32 = db.conn.query_row(
                    "SELECT \
                       (SELECT COUNT(*) FROM contexts     WHERE ts BETWEEN ?1 AND ?2) \
                     + (SELECT COUNT(*) FROM text_records WHERE ts BETWEEN ?1 AND ?2)",
                    params![start_ms, end_ms],
                    |r| r.get(0),
                )?;
                let mut stmt = db.conn.prepare(
                    "SELECT ts, local_time, tz_name, app, app_raw, 'ocr' AS kind, display, items, NULL AS text \
                       FROM contexts \
                       WHERE ts BETWEEN ?1 AND ?2 \
                     UNION ALL \
                     SELECT ts, local_time, tz_name, app, app_raw, 'keys' AS kind, NULL AS display, NULL AS items, text \
                       FROM text_records \
                       WHERE ts BETWEEN ?1 AND ?2 \
                     ORDER BY ts LIMIT ?3 OFFSET ?4",
                )?;
                let rows = stmt.query_map(
                    params![start_ms, end_ms, limit as i64, offset as i64],
                    row_to_timeline,
                )?;
                (total, rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
        };
        Ok(TimelinePage { total, records })
    })
}

/// Per-app usage over `[start_ms, end_ms]`, aggregated across both stores and
/// sorted by most recent activity first.
pub fn query_apps(start_ms: i64, end_ms: i64) -> Result<Vec<AppUsage>> {
    use std::collections::HashMap;
    with_db(|db| {
        let mut by_app: HashMap<String, AppUsage> = HashMap::new();

        // (is_context, sql) — same shape for both stores.
        for (is_context, sql) in [
            (
                false,
                "SELECT app, MAX(app_raw), MIN(ts), MAX(ts), COUNT(*) FROM text_records \
                 WHERE ts BETWEEN ?1 AND ?2 GROUP BY app",
            ),
            (
                true,
                "SELECT app, MAX(app_raw), MIN(ts), MAX(ts), COUNT(*) FROM contexts \
                 WHERE ts BETWEEN ?1 AND ?2 GROUP BY app",
            ),
        ] {
            let mut stmt = db.conn.prepare(sql)?;
            let rows = stmt.query_map(params![start_ms, end_ms], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, u32>(4)?,
                ))
            })?;
            for row in rows {
                let (app, app_raw, first, last, count) = row?;
                let usage = by_app.entry(app.clone()).or_insert_with(|| AppUsage {
                    app,
                    app_raw: app_raw.clone(),
                    first_ts: first,
                    last_ts: last,
                    text_records: 0,
                    context_records: 0,
                });
                usage.first_ts = usage.first_ts.min(first);
                usage.last_ts = usage.last_ts.max(last);
                if is_context {
                    usage.context_records += count;
                } else {
                    usage.text_records += count;
                }
                if usage.app_raw.is_empty() && !app_raw.is_empty() {
                    usage.app_raw = app_raw;
                }
            }
        }

        let mut out: Vec<AppUsage> = by_app.into_values().collect();
        out.sort_by_key(|u| std::cmp::Reverse(u.last_ts));
        Ok(out)
    })
}

/// Checkpoint the WAL and drop the connection (called on quit). Idempotent.
pub fn close() {
    let mut guard = slot().lock().unwrap();
    if let Some(db) = guard.take() {
        let _ = db.conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    }
}

// ---- helpers ----------------------------------------------------------------

/// Run `f` with the unlocked DB, or fail closed with "database is locked".
fn with_db<T>(f: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
    let guard = slot().lock().unwrap();
    let db = guard
        .as_ref()
        .ok_or_else(|| anyhow!("database is locked"))?;
    f(db)
}

/// Map a merged timeline row (see [`query_timeline`]) into a `TimelineRecord`.
/// Columns: 0 ts, 1 local_time, 2 tz_name, 3 app, 4 app_raw, 5 kind, 6 display,
/// 7 items(JSON), 8 text. Only the columns the row's `kind` populates are read;
/// a corrupt OCR items blob degrades to an empty item list rather than erroring.
fn row_to_timeline(r: &Row) -> rusqlite::Result<TimelineRecord> {
    let kind: String = r.get(5)?;
    let (kind, display, items, text) = if kind == "ocr" {
        let items_json: String = r.get(7)?;
        let items: Vec<OcrItem> = serde_json::from_str(&items_json).unwrap_or_default();
        (TimelineKind::Ocr, r.get::<_, Option<u32>>(6)?, items, None)
    } else {
        (
            TimelineKind::Keys,
            None,
            Vec::new(),
            r.get::<_, Option<String>>(8)?,
        )
    };
    Ok(TimelineRecord {
        ts: r.get(0)?,
        local_time: r.get(1)?,
        tz_name: r.get(2)?,
        app: r.get(3)?,
        app_raw: r.get(4)?,
        kind,
        display,
        items,
        text,
    })
}

/// Create tables/indices if missing (idempotent — runs on every open).
///
/// Every event carries three time columns so it stays interpretable after the
/// user crosses timezones: `ts` (epoch ms, the UTC instant, used for ordering),
/// `local_time` (local wall-clock without offset), and `tz_name` (IANA zone).
/// Every insert populates all three, so the text columns are plain `NOT NULL`.
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS text_records (
             ts         INTEGER NOT NULL,
             local_time TEXT    NOT NULL,
             tz_name    TEXT    NOT NULL,
             app        TEXT    NOT NULL,
             app_raw    TEXT    NOT NULL,
             text       TEXT    NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_text_ts ON text_records(ts);

         CREATE TABLE IF NOT EXISTS contexts (
             ts         INTEGER NOT NULL,
             local_time TEXT    NOT NULL,
             tz_name    TEXT    NOT NULL,
             app        TEXT    NOT NULL,
             app_raw    TEXT    NOT NULL,
             display    INTEGER NOT NULL,
             items      TEXT    NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_ctx_ts ON contexts(ts);

         CREATE TABLE IF NOT EXISTS meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )
    .context("running migrations")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_store::OcrItem;

    fn ctx(ts: i64, app: &str, texts: &[&str]) -> ContextRecord {
        ContextRecord {
            ts,
            local_time: "2026-07-05T12:00:00.000".to_string(),
            tz_name: "Asia/Shanghai".to_string(),
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

    fn rec(ts: i64, app: &str, text: &str) -> Record {
        Record {
            ts,
            local_time: "2026-07-05T12:00:00.000".to_string(),
            tz_name: "Asia/Shanghai".to_string(),
            app: app.to_string(),
            app_raw: app.to_string(),
            text: text.to_string(),
        }
    }

    /// Each test gets an isolated data dir; open/insert/query round-trips run
    /// against a real encrypted DB. Serialised because the connection slot is
    /// process-global.
    fn with_fresh_db(password: &str, body: impl FnOnce(&Path)) {
        static LOCK: Mutex<()> = Mutex::new(());
        let _serial = LOCK.lock().unwrap();
        close();
        let dir = std::env::temp_dir().join(format!(
            "pi0-db-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        open(&dir, password).unwrap();
        body(&dir);
        close();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_is_password_protected() {
        with_fresh_db("correct-horse", |dir| {
            insert_text_record(&rec(1000, "Chrome", "hello")).unwrap();
            close();
            // Wrong password is rejected.
            assert!(open(dir, "wrong").is_err());
            // Right password unlocks and the data is intact.
            assert!(!open(dir, "correct-horse").unwrap(), "not newly created");
            let rows = query_text(0, 2000).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].text, "hello");
        });
    }

    #[test]
    fn timeline_merges_paginates_and_apps_aggregate() {
        with_fresh_db("pw", |_dir| {
            insert_context(&ctx(1000, "Chrome", &["hello", "world"])).unwrap();
            insert_context(&ctx(1010, "Chrome", &["again"])).unwrap();
            insert_context(&ctx(1020, "Lark", &["message"])).unwrap();
            insert_text_record(&rec(1005, "Chrome", "typed")).unwrap();

            // Contexts and keystrokes interleave by ts in one paginated stream:
            // ts 1000 ocr, 1005 keys, 1010 ocr, 1020 ocr.
            let page = query_timeline(0, 5000, None, 0, 2).unwrap();
            assert_eq!(page.total, 4);
            assert_eq!(page.records.len(), 2);
            assert_eq!(page.records[0].kind, TimelineKind::Ocr);
            assert_eq!(page.records[0].items[0].text, "hello");
            assert_eq!(page.records[1].kind, TimelineKind::Keys);
            assert_eq!(page.records[1].text.as_deref(), Some("typed"));

            let page2 = query_timeline(0, 5000, None, 2, 2).unwrap();
            assert_eq!(page2.records.len(), 2);
            assert_eq!(page2.records[0].kind, TimelineKind::Ocr); // ts 1010 Chrome
            assert_eq!(page2.records[1].app, "Lark"); // ts 1020

            // App filter is case-insensitive and spans both stores.
            let chrome = query_timeline(0, 5000, Some("CHROME"), 0, 10).unwrap();
            assert_eq!(chrome.total, 3, "2 contexts + 1 keystroke record");
            let lark = query_timeline(0, 5000, Some("LARK"), 0, 10).unwrap();
            assert_eq!(lark.total, 1);

            let apps = query_apps(0, 5000).unwrap();
            assert_eq!(apps.len(), 2);
            assert_eq!(apps[0].app, "Lark", "most recent first");
            let chrome_usage = apps.iter().find(|a| a.app == "Chrome").unwrap();
            assert_eq!(chrome_usage.context_records, 2);
            assert_eq!(chrome_usage.text_records, 1);
        });
    }

    #[test]
    fn rekey_then_reopen_with_new_password() {
        with_fresh_db("old-pw", |dir| {
            insert_text_record(&rec(1, "App", "x")).unwrap();
            assert!(change_password("wrong", "new-pw").is_err());
            change_password("old-pw", "new-pw").unwrap();
            close();
            assert!(open(dir, "old-pw").is_err(), "old password no longer works");
            open(dir, "new-pw").unwrap();
            assert_eq!(query_text(0, 10).unwrap().len(), 1);
        });
    }

    #[test]
    fn mcp_token_is_stable() {
        with_fresh_db("pw", |_dir| {
            let a = mcp_token().unwrap();
            let b = mcp_token().unwrap();
            assert_eq!(a, b, "token is minted once and reused");
            assert_eq!(a.len(), 64, "32 random bytes as hex");
        });
    }
}
