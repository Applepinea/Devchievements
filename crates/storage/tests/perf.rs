//! Preliminary performance benchmark (spec 31.3).
//!
//! Marked `#[ignore]` so it does not run in the normal suite: it writes thousands of
//! rows and would slow every build. Run it explicitly with
//! `cargo test -p devchievements-storage --release --test perf -- --ignored --nocapture`.
//!
//! # What is measured, and how
//!
//! End-to-end ingestion through the real SQLite path: validate, deduplicate, persist
//! the event, update projections, evaluate the Engine, queue outbox rows, commit.
//! That is the whole path a Hook event takes once it reaches the Companion, so the
//! number is meaningful rather than a micro-benchmark of one function.
//!
//! Reported: n, min, p50, p95, max, plus events/second. Targets in spec 44.1 are for
//! the Hook (p95 < 50 ms) and Commit-to-notification (p95 < 500 ms); this benchmark
//! covers the storage+engine portion of the latter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_core::{FixedClock, IdGenerator, InjectedContext, TimezoneProvider};
use devchievements_events::{
    CanonicalRepositoryId, EmailOrigin, Event, EventId, EventSource, EventType, Identity,
    LocalDate, SchemaVersion, SourceConfidence,
};
use devchievements_storage::ingest::{self, IngestOutcome};

#[derive(Debug)]
struct Utc;
impl TimezoneProvider for Utc {
    fn canonicalize(&self, n: &str) -> Option<String> {
        Some(n.to_string())
    }
    fn local_date(&self, ms: i64, _z: &str, d: u16) -> Option<String> {
        devchievements_core::local_date_at(
            ms,
            "UTC",
            devchievements_core::DayBoundary::from_minutes(d).ok()?,
        )
    }
}

#[derive(Debug, Default)]
struct Ids(u64);
impl IdGenerator for Ids {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

#[derive(Debug)]
struct Cfg;
impl devchievements_core::ConfigurationProvider for Cfg {
    fn config_version(&self) -> u64 {
        1
    }
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * sorted.len() as f64).floor() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[tokio::test]
#[ignore = "performance benchmark; run explicitly with --ignored"]
async fn ingest_throughput() {
    const N: usize = 2_000;

    let dir = std::env::temp_dir().join(format!("devch-perf-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let pool = devchievements_storage::open(&dir.join("perf.db"))
        .await
        .expect("open");
    ingest::migrate(&pool).await.expect("migrate");

    let clock = FixedClock(1_700_000_000_000);
    let tz = Utc;
    let mut ids = Ids::default();
    let cfg = Cfg;
    let ctx = InjectedContext {
        clock: &clock,
        timezone: &tz,
        ids: &mut ids,
        config: &cfg,
    };

    // The identity gate is part of the measured path now, so the benchmark includes it.
    let mut identity = Identity::default();
    identity.add_email("perf@devchievements.local", EmailOrigin::UserConfigured);

    let facts = ingest::commit_facts(1, false);
    let mut samples = Vec::with_capacity(N);

    for i in 0..N {
        // Spread across days so the daily projection also gets exercised.
        let occurred = 1_700_000_000_000i64 + (i as i64) * 60_000;
        let date = devchievements_core::local_date_at(
            occurred,
            "UTC",
            devchievements_core::DayBoundary::MIDNIGHT,
        )
        .expect("date");

        let event = Event {
            event_id: EventId::new(format!("perf-{i}")).expect("id"),
            event_type: EventType::CommitCreated,
            source: EventSource::LocalGitHook,
            source_confidence: SourceConfidence::VerifiedLocal,
            canonical_repository_id: CanonicalRepositoryId("github:perf/r".to_string()),
            external_id: None,
            commit_sha: Some(format!("{i:040x}")),
            occurred_at: occurred,
            observed_at: occurred,
            received_at: occurred,
            local_date: LocalDate::new(date).expect("date"),
            timezone: "UTC".to_string(),
            date_policy: 0,
            payload: format!("{{\"author_email\":\"{}\"}}", "perf@devchievements.local"),
            schema_version: SchemaVersion(1),
        };

        let t0 = std::time::Instant::now();
        let outcome = ingest::ingest_commit(&pool, &event, &facts, occurred, &identity, &ctx)
            .await
            .expect("ingest");
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);

        assert!(
            matches!(
                outcome,
                IngestOutcome::Ingested { .. } | IngestOutcome::IngestedSilently
            ),
            "every event in this benchmark is new"
        );
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let total: f64 = samples.iter().sum();
    let n = samples.len();

    println!("\n=== ingest benchmark (n={n}) ===");
    println!("min  {:.3} ms", samples[0]);
    println!("p50  {:.3} ms", pct(&samples, 50.0));
    println!("p95  {:.3} ms", pct(&samples, 95.0));
    println!("max  {:.3} ms", samples[n - 1]);
    println!("mean {:.3} ms", total / n as f64);
    println!("throughput {:.0} events/sec", (n as f64) / (total / 1000.0));
    println!(
        "\nContext: spec 44.1 target for Hook p95 is < 50 ms and Commit->notification p95 < 500 ms.\n\
         This measures the storage+engine portion of the latter, on this machine, in release mode.\n\
         It is NOT a claim that the end-to-end target is met: there is no Companion/notification\n\
         layer yet."
    );

    // Sanity: the data really landed.
    assert_eq!(ingest::event_count(&pool).await.expect("count"), N as i64);
    std::fs::remove_dir_all(&dir).ok();
}
