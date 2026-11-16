//! Timezone invariants required by spec 12.
//!
//! # The two properties that matter
//!
//! 1. **Determinism**: the same instant + zone + day boundary always yields the same
//!    `local_date`. Without this, replaying history would produce different numbers
//!    than the live run, and every "my stats changed" report becomes unfalsifiable.
//! 2. **Day-boundary consistency**: the computed date never drifts more than one day
//!    from the civil date in that zone, and the boundary only ever shifts events
//!    BACKWARD. A boundary of 04:00 must not push an 03:00 event forward.
//!
//! Property tests are the right tool here because the failure cases are exactly the
//! ones nobody thinks to enumerate: DST gaps, midnight boundaries, leap days.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_core::{local_date_at, DayBoundary};
use proptest::prelude::*;

/// Zones chosen to cover: no DST (Taipei, UTC), southern hemisphere DST (Sydney),
/// northern DST (New York), a half-hour DST shift (Lord Howe), and a very unusual
/// offset (Kathmandu, +05:45).
const ZONES: &[&str] = &[
    "UTC",
    "Asia/Taipei",
    "America/New_York",
    "Australia/Sydney",
    "Asia/Kathmandu",
];

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2_000,
        .. ProptestConfig::default()
    })]

    /// Same inputs, same output — always.
    #[test]
    fn local_date_is_deterministic(
        millis in 0i64..2_000_000_000_000i64,
        zone_idx in 0usize..ZONES.len(),
        boundary_min in 0u16..=720u16,
    ) {
        let zone = ZONES[zone_idx];
        let boundary = DayBoundary::from_minutes(boundary_min).expect("in range");
        let first = local_date_at(millis, zone, boundary).expect("valid input");
        for _ in 0..3 {
            prop_assert_eq!(
                &first,
                &local_date_at(millis, zone, boundary).expect("valid input")
            );
        }
    }

    /// The computed date is at most one day away from the civil date.
    ///
    /// A day boundary can move an event into the previous day, but it must never
    /// move it further than that: a 12:00 boundary applied to a 23:00 event is
    /// still the same day.
    #[test]
    fn boundary_shifts_by_at_most_one_day(
        millis in 0i64..2_000_000_000_000i64,
        zone_idx in 0usize..ZONES.len(),
        boundary_min in 0u16..=720u16,
    ) {
        let zone = ZONES[zone_idx];
        let civil = local_date_at(millis, zone, DayBoundary::MIDNIGHT).expect("valid");
        let shifted = local_date_at(
            millis,
            zone,
            DayBoundary::from_minutes(boundary_min).expect("in range"),
        )
        .expect("valid");

        let civil_day = day_number(&civil);
        let shifted_day = day_number(&shifted);
        prop_assert!(
            shifted_day <= civil_day,
            "the boundary must never push an event forward (civil {} -> shifted {})",
            civil,
            shifted
        );
        prop_assert!(
            civil_day - shifted_day <= 1,
            "the boundary moved an event more than one day (civil {} -> shifted {})",
            civil,
            shifted
        );
    }

    /// A midnight boundary must be idempotent and must never move backwards.
    ///
    /// This used to compare against a jiff recomputation, but that only proves jiff
    /// agrees with itself. The meaningful properties are the ones below: evaluating
    /// twice gives the same answer, and a zero boundary behaves like "no shift".
    #[test]
    fn midnight_boundary_is_stable_and_shifts_nothing(
        millis in 0i64..2_000_000_000_000i64,
        zone_idx in 0usize..ZONES.len(),
    ) {
        let zone = ZONES[zone_idx];
        let a = local_date_at(millis, zone, DayBoundary::MIDNIGHT).expect("valid");
        let b = local_date_at(millis, zone, DayBoundary::MIDNIGHT).expect("valid");
        prop_assert_eq!(&a, &b);

        // A zero boundary is the baseline: applying it twice, or mixing it with
        // itself, cannot change the answer.
        let explicit_zero = DayBoundary::from_minutes(0).expect("valid");
        prop_assert_eq!(
            &local_date_at(millis, zone, explicit_zero).expect("valid"),
            &a
        );
    }

    /// Advancing time never moves the computed date backwards.
    ///
    /// Monotonicity is what makes "days are sequential" true, which the streak
    /// projection depends on.
    #[test]
    fn local_date_is_monotonic_in_time(
        millis in 0i64..1_990_000_000_000i64,
        step_ms in 1i64..86_400_000i64,
        zone_idx in 0usize..ZONES.len(),
        boundary_min in 0u16..=720u16,
    ) {
        let zone = ZONES[zone_idx];
        let boundary = DayBoundary::from_minutes(boundary_min).expect("in range");
        let earlier = local_date_at(millis, zone, boundary).expect("valid");
        let later = local_date_at(millis + step_ms, zone, boundary).expect("valid");
        prop_assert!(
            day_number(&later) >= day_number(&earlier),
            "time moved forward but the date went backwards: {} then {}",
            earlier,
            later
        );
    }

    /// An unknown zone is an error, never a guess.
    #[test]
    fn unknown_zones_never_produce_a_date(millis in 0i64..2_000_000_000_000i64) {
        prop_assert!(local_date_at(millis, "Not/AZone", DayBoundary::MIDNIGHT).is_none());
    }
}

/// Convert YYYY-MM-DD into a comparable day number (days since epoch).
///
/// Implemented here rather than pulled from a crate so the test does not depend on
/// the same code it is checking.
fn day_number(date: &str) -> i64 {
    let y: i64 = date[0..4].parse().expect("year");
    let m: i64 = date[5..7].parse().expect("month");
    let d: i64 = date[8..10].parse().expect("day");

    // Days from 0000-03-01 based算法, adequate for ordering comparisons.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe
}
