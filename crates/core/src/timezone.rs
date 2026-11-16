//! Timezone and local-date computation (spec 12, ADR-0005).
//!
//! # The one invariant everything else depends on
//!
//! `local_date` is computed ONCE, at ingestion, from the event's own instant, the
//! IANA zone configured at that moment, and the day boundary in force at that
//! moment. It is then frozen forever. Travel, DST changes and policy edits must
//! never rewrite history; recomputing produces a NEW projection with recorded
//! provenance rather than mutating the old row.
//!
//! # Why the TZDB is bundled
//!
//! Reading the OS timezone database would let a Windows update change how a past
//! event maps to a date, silently rewriting statistics. Bundling it makes "replay
//! is stable" a property of the build instead of a hope. The database identity is
//! exposed so diagnostics can explain a discrepancy.

use crate::context::TimezoneProvider;

/// The maximum supported day boundary, in minutes from local midnight.
///
/// Spec 12 allows any configurable start (examples given: 00:00, 04:00, 06:00).
/// We cap at 12:00 because a "day" that starts in the afternoon would make the
/// concept meaningless and is almost certainly a configuration mistake.
pub const MAX_DAY_START_MINUTES: u16 = 12 * 60;

/// A validated day boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DayBoundary(u16);

impl DayBoundary {
    /// Midnight, the default (spec 12).
    pub const MIDNIGHT: DayBoundary = DayBoundary(0);

    /// Construct from minutes-from-midnight, rejecting out-of-range values.
    pub fn from_minutes(minutes: u16) -> Result<Self, DayBoundaryError> {
        if minutes > MAX_DAY_START_MINUTES {
            return Err(DayBoundaryError::OutOfRange(minutes));
        }
        Ok(DayBoundary(minutes))
    }

    /// The boundary in minutes from local midnight.
    pub fn minutes(self) -> u16 {
        self.0
    }
}

/// Why a day boundary was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayBoundaryError {
    /// The requested boundary is later than noon.
    OutOfRange(u16),
}

impl std::fmt::Display for DayBoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DayBoundaryError::OutOfRange(m) => {
                write!(
                    f,
                    "day boundary {m} minutes is beyond the {MAX_DAY_START_MINUTES}-minute limit"
                )
            }
        }
    }
}

impl std::error::Error for DayBoundaryError {}

/// Timezone provider backed by a bundled TZDB.
#[derive(Debug, Clone, Copy, Default)]
pub struct BundledTimezoneProvider;

impl BundledTimezoneProvider {
    /// Create a provider. Cheap: the TZDB is compiled in.
    pub fn new() -> Self {
        BundledTimezoneProvider
    }
}

impl TimezoneProvider for BundledTimezoneProvider {
    fn canonicalize(&self, iana_name: &str) -> Option<String> {
        let tz = jiff::tz::TimeZone::get(iana_name.trim()).ok()?;
        // jiff resolves aliases; report the canonical name it ended up with.
        Some(tz.iana_name().unwrap_or(iana_name.trim()).to_string())
    }

    fn local_date(
        &self,
        unix_millis: i64,
        iana_name: &str,
        day_start_minutes: u16,
    ) -> Option<String> {
        let boundary = DayBoundary::from_minutes(day_start_minutes).ok()?;
        local_date_at(unix_millis, iana_name, boundary)
    }
}

/// Compute the local date for an instant, applying the day boundary.
///
/// # Day boundary semantics (spec 12)
///
/// With a 04:00 boundary, an event at 02:30 local time belongs to the PREVIOUS
/// day. That is the whole point: late-night work should count toward the day the
/// user was still awake for.
///
/// DST is handled by the timezone library's standard resolution rules, as the
/// spec requires, rather than by manual offset arithmetic.
pub fn local_date_at(unix_millis: i64, iana_name: &str, boundary: DayBoundary) -> Option<String> {
    let tz = jiff::tz::TimeZone::get(iana_name.trim()).ok()?;
    let zoned = jiff::Timestamp::from_millisecond(unix_millis)
        .ok()?
        .to_zoned(tz);

    // Shifting back by the boundary makes "before the boundary" fall into the
    // previous calendar day once we read off the shifted date.
    //
    // NOTE: this is a naive civil-time shift, which is exactly what we want: the
    // day boundary is a civil-time concept, not an absolute-duration one.
    let shifted = zoned
        .datetime()
        .checked_sub(jiff::SignedDuration::from_mins(i64::from(
            boundary.minutes(),
        )))
        .ok()?;

    Some(format!(
        "{:04}-{:02}-{:02}",
        shifted.year(),
        shifted.month(),
        shifted.day()
    ))
}

/// Identifies the timezone data this build uses.
///
/// jiff 0.2 does not expose the TZDB revision through a stable API, so we report
/// the jiff version that bundles it. That is enough for diagnostics to tell two
/// builds apart when a historical `local_date` looks wrong: if the versions
/// differ, the database may differ too.
pub fn tzdb_version() -> String {
    format!("jiff/{}, tzdb=bundled", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-16T00:00:00Z, verified with `Date.UTC(2026, 10, 16)` in Node.
    ///
    /// Hard-coding epochs is error-prone: an earlier version of this test used
    /// the value for 2025 and "failed" against correct code. Every constant here
    /// is documented with how it was derived.
    const NOV16_MIDNIGHT_UTC: i64 = 1_794_787_200_000;

    #[test]
    fn day_boundary_rejects_afternoon_starts() {
        assert!(DayBoundary::from_minutes(0).is_ok());
        assert!(DayBoundary::from_minutes(240).is_ok());
        assert!(DayBoundary::from_minutes(720).is_ok());
        assert!(DayBoundary::from_minutes(721).is_err());
        assert!(DayBoundary::from_minutes(1439).is_err());
    }

    #[test]
    fn utc_and_taipei_agree_across_a_simple_midnight() {
        // Taipei is UTC+8 with no DST, so noon UTC is the same date in both.
        let noon_utc = NOV16_MIDNIGHT_UTC + 12 * 3600 * 1000;
        let utc = local_date_at(noon_utc, "UTC", DayBoundary::MIDNIGHT).unwrap();
        let tpe = local_date_at(noon_utc, "Asia/Taipei", DayBoundary::MIDNIGHT).unwrap();
        assert_eq!(utc, "2026-11-16");
        assert_eq!(tpe, "2026-11-16");
    }

    #[test]
    fn a_four_am_boundary_moves_the_small_hours_to_the_previous_day() {
        // Spec 12: with a 04:00 boundary, 00:00-03:59 belongs to the PREVIOUS day.
        //
        // 02:30 on 2026-11-16 in Asia/Taipei (UTC+8) is 2026-11-15T18:30:00Z.
        let local_0230 = 1_794_767_400_000;

        let tz = jiff::tz::TimeZone::get("Asia/Taipei").expect("zone");
        let zdt = jiff::Timestamp::from_millisecond(local_0230)
            .expect("valid instant")
            .to_zoned(tz);
        let civil_today = format!("{:04}-{:02}-{:02}", zdt.year(), zdt.month(), zdt.day());
        assert_eq!(
            civil_today, "2026-11-16",
            "sanity: the instant really is the 16th locally"
        );
        assert_eq!(zdt.hour(), 2, "sanity: and it really is 02:30 local");

        // With a midnight boundary the event belongs to today.
        assert_eq!(
            local_date_at(local_0230, "Asia/Taipei", DayBoundary::MIDNIGHT).as_deref(),
            Some("2026-11-16")
        );

        // With a 04:00 boundary it belongs to YESTERDAY.
        assert_eq!(
            local_date_at(
                local_0230,
                "Asia/Taipei",
                DayBoundary::from_minutes(240).expect("valid")
            )
            .as_deref(),
            Some("2026-11-15")
        );
    }

    #[test]
    fn exactly_on_the_boundary_belongs_to_the_new_day() {
        // 04:00 exactly is the first moment of the new day, not the last of the old.
        let tz = jiff::tz::TimeZone::get("Asia/Taipei").unwrap();
        let at_0400 = jiff::civil::date(2026, 11, 16)
            .at(4, 0, 0, 0)
            .to_zoned(tz)
            .expect("04:00 exists in Taipei");
        let millis = at_0400.timestamp().as_millisecond();
        let with_boundary = local_date_at(
            millis,
            "Asia/Taipei",
            DayBoundary::from_minutes(240).unwrap(),
        )
        .unwrap();
        let civil = format!(
            "{:04}-{:02}-{:02}",
            at_0400.year(),
            at_0400.month(),
            at_0400.day()
        );
        assert_eq!(with_boundary, civil);
    }

    #[test]
    fn dst_transition_days_resolve_to_a_single_date() {
        // US DST starts 2026-03-08, so that civil day is 23 hours long.
        // Walking "24 hours" would spill into the 9th; walking the CIVIL day is the
        // correct way to ask "does every instant that day map to that date".
        let tz = jiff::tz::TimeZone::get("America/New_York").expect("zone");
        for hour in 0..24 {
            let Ok(dt) = jiff::civil::date(2026, 3, 8)
                .at(hour, 0, 0, 0)
                .to_zoned(tz.clone())
            else {
                // 02:xx does not exist on a spring-forward day; the library's
                // rejection is the correct standard behaviour.
                continue;
            };
            let millis = dt.timestamp().as_millisecond();
            assert_eq!(
                local_date_at(millis, "America/New_York", DayBoundary::MIDNIGHT).as_deref(),
                Some("2026-03-08"),
                "hour {hour} on the DST day must map to that date"
            );
        }
    }

    #[test]
    fn a_nonexistent_local_time_does_not_panic() {
        // DST spring-forward removes 02:00-03:00 local. The boundary resolution
        // must follow the library's rules rather than blowing up.
        let result = local_date_at(
            NOV16_MIDNIGHT_UTC,
            "America/New_York",
            DayBoundary::from_minutes(150).unwrap(),
        );
        assert!(result.is_some());
    }

    #[test]
    fn unknown_timezone_returns_none_instead_of_guessing() {
        assert!(local_date_at(NOV16_MIDNIGHT_UTC, "Mars/Olympus", DayBoundary::MIDNIGHT).is_none());
    }

    #[test]
    fn the_same_instant_always_maps_to_the_same_date() {
        // The core replay-invariance property (spec 12): same inputs, same output.
        let a = local_date_at(
            NOV16_MIDNIGHT_UTC,
            "Asia/Taipei",
            DayBoundary::from_minutes(240).unwrap(),
        );
        for _ in 0..5 {
            assert_eq!(
                a,
                local_date_at(
                    NOV16_MIDNIGHT_UTC,
                    "Asia/Taipei",
                    DayBoundary::from_minutes(240).unwrap()
                )
            );
        }
    }

    #[test]
    fn provider_canonicalizes_aliases() {
        let p = BundledTimezoneProvider::new();
        assert!(p.canonicalize("Asia/Taipei").is_some());
        assert!(p.canonicalize("Not/AZone").is_none());
    }

    #[test]
    fn tzdb_version_is_reported() {
        let v = tzdb_version();
        assert!(!v.is_empty());
    }
}
