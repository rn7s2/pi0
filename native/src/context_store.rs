//! OCR context types shared between the OCR thread (which produces contexts),
//! the store ([`crate::db`]), and the query surface. Persistence and querying
//! live in [`crate::db`]; this module is just the shapes.

use serde::{Deserialize, Serialize};

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
