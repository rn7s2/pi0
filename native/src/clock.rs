//! Time stamping. Every recorded event is tagged with a [`Stamp`] — the trio of
//! UTC instant, local wall-clock, and IANA timezone name — so that times stay
//! correct even after the user moves across timezones (a local-only or
//! offset-only stamp becomes ambiguous the moment the zone changes).
//!
//! Two clocks live here on purpose:
//! - [`now_ms`] is a bare epoch-ms read (a `clock_gettime`, no timezone work),
//!   cheap enough to call on every high-frequency input event (mouse movement).
//! - [`stamp`] additionally resolves the local wall-clock and zone name; it runs
//!   only at record-creation cadence (a handful of times per minute), so its
//!   heavier timezone lookup is never on the hot path.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};

/// ISO-8601 local wall-clock, millisecond precision, **no** offset suffix — e.g.
/// `2026-07-05T14:30:00.123`. The offset is intentionally omitted: the zone is
/// carried separately in [`Stamp::tz_name`].
const LOCAL_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f";

/// Epoch milliseconds (the UTC instant). Cheap: a single system-clock read with
/// no timezone resolution, safe to call per input event.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A fully-resolved instant stored on every record: the UTC epoch ms, the local
/// wall-clock (without offset), and the IANA timezone name it was captured in.
#[derive(Debug, Clone)]
pub struct Stamp {
    /// Epoch milliseconds — the UTC instant, unambiguous and used for ordering.
    pub ts: i64,
    /// Local wall-clock without offset (see [`LOCAL_FORMAT`]).
    pub local_time: String,
    /// IANA timezone name, e.g. `Asia/Shanghai`. Falls back to the numeric
    /// offset (e.g. `+08:00`) if the zone name can't be resolved.
    pub tz_name: String,
}

/// Stamp "now" — resolves the current UTC instant, local time, and zone name in
/// one shot so the three stay mutually consistent.
pub fn stamp() -> Stamp {
    let now = Local::now();
    Stamp {
        ts: now.timestamp_millis(),
        local_time: now.naive_local().format(LOCAL_FORMAT).to_string(),
        tz_name: tz_name(&now),
    }
}

/// Reconstruct a [`Stamp`] from a known epoch ms using the *current* local zone.
/// Used only for crash-recovery sweeps, where a leftover screenshot's filename
/// preserves its `ts` but not the local time / zone it was captured in — the
/// current zone is the best available approximation for a recent shot.
pub fn stamp_from_ms(ts: i64) -> Stamp {
    let local = Local
        .timestamp_millis_opt(ts)
        .single()
        .unwrap_or_else(Local::now);
    Stamp {
        ts,
        local_time: local.naive_local().format(LOCAL_FORMAT).to_string(),
        tz_name: tz_name(&local),
    }
}

/// The IANA zone name for the machine, falling back to the datetime's numeric
/// offset when the name is unavailable (never fails).
fn tz_name(fallback_from: &chrono::DateTime<Local>) -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| fallback_from.offset().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_resolves_all_three_parts() {
        let s = stamp();
        assert!(s.ts > 1_600_000_000_000, "ts looks like epoch ms: {}", s.ts);
        // Local wall-clock is ISO-shaped and carries NO offset (the zone name is
        // separate) — that separation is the whole point of the trio.
        assert!(
            s.local_time.contains('T') && s.local_time.len() >= 19,
            "local_time shape: {}",
            s.local_time
        );
        assert!(
            !s.local_time.ends_with('Z') && !s.local_time.contains('+'),
            "local_time must not carry an offset: {}",
            s.local_time
        );
        assert!(!s.tz_name.is_empty(), "tz name resolved");
    }

    #[test]
    fn stamp_from_ms_preserves_ts() {
        let s = stamp_from_ms(1_751_527_334_123);
        assert_eq!(s.ts, 1_751_527_334_123);
        assert!(s.local_time.contains('T'), "local_time: {}", s.local_time);
        assert!(!s.tz_name.is_empty(), "tz name resolved");
    }
}
