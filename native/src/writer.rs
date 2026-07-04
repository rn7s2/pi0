//! The keystroke-record type. Persistence lives in [`crate::db`]; this module
//! is just the shape shared between the HID thread (which produces records) and
//! the store.

use serde::{Deserialize, Serialize};

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
