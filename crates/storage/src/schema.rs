//! The Phase 1 schema (spec 4.1, 4.3, 13.1, 14.2, 16.1).
//!
//! # Shape decisions that matter
//!
//! - **Raw events and derived statistics are separate tables.** An event is a fact;
//!   a projection is a cached aggregate. Mixing them makes "rebuild the projection"
//!   impossible without losing data (spec 4.1).
//! - **`unlocked_achievements` and `daily_milestones` are separate tables.** Spec 13.1
//!   requires it explicitly, because one is permanent-once and the other repeats
//!   daily. A single table would force one of the two semantics onto the other.
//! - **The outbox lives in the same database**, so an event, its projection updates
//!   and its notification intents commit in ONE transaction (spec 16.1).
//! - **Every projection row carries the policy that produced it** (timezone, day
//!   boundary, rule version), so a later review can explain why a number changed.

/// Version applied by migration 1.
pub const SCHEMA_VERSION: i64 = 1;

/// The complete Phase 1 schema as plain SQL.
///
/// Plain SQL rather than `sqlx::migrate!` because Phase 1 still has only one
/// version; the moment there is a second, this becomes a numbered migration and
/// this constant becomes migration 001.
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

-- ---------------------------------------------------------------- raw events
CREATE TABLE IF NOT EXISTS events (
    event_id                TEXT PRIMARY KEY NOT NULL,
    event_type              TEXT NOT NULL,
    source                  TEXT NOT NULL,
    source_confidence       TEXT NOT NULL,
    canonical_repository_id TEXT NOT NULL,
    external_id             TEXT,
    commit_sha              TEXT,
    occurred_at             INTEGER NOT NULL,
    observed_at             INTEGER NOT NULL,
    received_at             INTEGER NOT NULL,
    local_date              TEXT NOT NULL,
    timezone                TEXT NOT NULL,
    date_policy             INTEGER NOT NULL,
    payload                 TEXT NOT NULL,
    schema_version          INTEGER NOT NULL,
    -- Provenance of an imported/replayed event, for diagnostics.
    ingest_batch            TEXT
);

-- Dedup key for commit-like events (spec 6.2): repository + sha.
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_repo_sha
    ON events (canonical_repository_id, commit_sha)
    WHERE commit_sha IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_events_repo_date
    ON events (canonical_repository_id, local_date);

CREATE INDEX IF NOT EXISTS idx_events_occurred
    ON events (occurred_at);

-- ------------------------------------------------------------- projections
-- One row per (repository, local_date). Everything the Engine reads for a commit.
CREATE TABLE IF NOT EXISTS daily_projection (
    canonical_repository_id TEXT NOT NULL,
    local_date              TEXT NOT NULL,
    commit_count            INTEGER NOT NULL DEFAULT 0,
    daily_rank              TEXT,
    -- The timezone and day boundary in force when this row was written. Recorded
    -- so a discrepancy can be explained rather than argued about (spec 12).
    timezone                TEXT NOT NULL,
    date_policy             INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    PRIMARY KEY (canonical_repository_id, local_date)
);

-- Repository-wide counters, kept incrementally so a commit never scans history.
CREATE TABLE IF NOT EXISTS repository_projection (
    canonical_repository_id TEXT PRIMARY KEY NOT NULL,
    commit_count            INTEGER NOT NULL DEFAULT 0,
    first_seen_at           INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL
);

-- ------------------------------------------------------- permanent achievements
CREATE TABLE IF NOT EXISTS unlocked_achievements (
    achievement_id  TEXT PRIMARY KEY NOT NULL,
    unlocked_at     INTEGER NOT NULL,
    -- realtime | delayed | backfill | import (spec 13.4)
    unlock_source   TEXT NOT NULL,
    rule_version    INTEGER NOT NULL,
    viewed_at       INTEGER,
    -- The event that caused it, when there was one.
    triggering_event_id TEXT
);

-- ------------------------------------------------------------ daily milestones
-- Repeatable per day, so the key is (milestone, date) (spec 14.2).
CREATE TABLE IF NOT EXISTS daily_milestones (
    milestone_id   TEXT NOT NULL,
    local_date     TEXT NOT NULL,
    threshold      INTEGER NOT NULL,
    reached_at     INTEGER NOT NULL,
    viewed_at      INTEGER,
    PRIMARY KEY (milestone_id, local_date)
);

-- ------------------------------------------------------------------ outbox
CREATE TABLE IF NOT EXISTS outbox (
    notification_id     TEXT PRIMARY KEY NOT NULL,
    event_id            TEXT NOT NULL,
    status              TEXT NOT NULL,
    owner_channel       TEXT,
    claimer_id          TEXT,
    offer_deadline      INTEGER,
    created_at          INTEGER NOT NULL,
    attempts            INTEGER NOT NULL DEFAULT 0,
    last_error          TEXT,
    processed_at        INTEGER,
    -- Persisted BEFORE the desktop shows anything (spec 17.9).
    display_attempted_at INTEGER,
    -- What to show: kind plus a JSON payload, so the consumer needs no extra query.
    kind                TEXT NOT NULL,
    payload             TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (event_id) REFERENCES events (event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_outbox_status ON outbox (status);
CREATE INDEX IF NOT EXISTS idx_outbox_created ON outbox (created_at);

-- --------------------------------------------------------- import bookkeeping
-- Makes history import resumable and idempotent (spec 11.2).
CREATE TABLE IF NOT EXISTS import_batches (
    batch_id    TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    -- pending | completed | cancelled
    status      TEXT NOT NULL,
    imported_events INTEGER NOT NULL DEFAULT 0,
    skipped_duplicates INTEGER NOT NULL DEFAULT 0
);
"#;
