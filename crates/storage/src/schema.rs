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

/// Version applied by the current schema.
///
/// Bumped to 2 when the browser-activity counters were added to
/// `repository_projection`, to 3 for `browser_reports_rejected`, and to 4 for
/// `daily_global_projection` + `schema_meta.global_commit_total`, and to 5 for
/// per-event repeatable achievement triggers.
///
/// Version 1-4 databases are upgraded in place by `ingest::migrate`: new columns come
/// from [`COLUMN_UPGRADES`], new tables from [`SCHEMA_SQL`] (`CREATE TABLE IF NOT
/// EXISTS`), and the new global projection is backfilled from the per-repository rows
/// that are already correct. Nothing is dropped, so no event and no unlock is lost.
pub const SCHEMA_VERSION: i64 = 5;

/// Columns added after version 1, as `(table, column, SQL type)`.
///
/// `CREATE TABLE IF NOT EXISTS` cannot add a column to an existing table, and this
/// product has users on version 1 already: their database must gain the counters
/// rather than be rejected or rebuilt.
pub const COLUMN_UPGRADES: &[(&str, &str, &str)] = &[
    (
        "repository_projection",
        "pull_request_merged_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "repository_projection",
        "release_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "repository_projection",
        "issue_opened_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "repository_projection",
        "repositories_created_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "repository_projection",
        "star_given_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
];

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

-- One row per LOCAL DATE across every repository (spec 14.1, checklist T1).
--
-- Why this table exists at all: the daily rank is about the user's day, not about one
-- repository's day. Three commits in A, two in B and one in C is a six-commit day, and
-- six is Rare. Reading per-repository rows and taking one of them made it Common.
--
-- Kept as its own table rather than a `canonical_repository_id = ''` row in
-- `daily_projection`: that table's primary key includes the repository, so a sentinel
-- would make every reader guess whether an id is real, and the Repositories page would
-- have to filter it out.
CREATE TABLE IF NOT EXISTS daily_global_projection (
    local_date   TEXT PRIMARY KEY NOT NULL,
    commit_count INTEGER NOT NULL DEFAULT 0,
    daily_rank   TEXT,
    -- The policy in force when the row was written, same reason as above (spec 12).
    timezone     TEXT NOT NULL,
    date_policy  INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

-- Repository-wide counters, kept incrementally so a commit never scans history.
CREATE TABLE IF NOT EXISTS repository_projection (
    canonical_repository_id TEXT PRIMARY KEY NOT NULL,
    commit_count            INTEGER NOT NULL DEFAULT 0,
    -- GitHub activities observed by the browser extension (spec 2.3). Kept per
    -- repository because that is where the activity happened; the achievements read
    -- the sum, which is the user's total.
    pull_request_merged_count     INTEGER NOT NULL DEFAULT 0,
    release_count                 INTEGER NOT NULL DEFAULT 0,
    issue_opened_count            INTEGER NOT NULL DEFAULT 0,
    repositories_created_count    INTEGER NOT NULL DEFAULT 0,
    star_given_count              INTEGER NOT NULL DEFAULT 0,
    first_seen_at           INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL
);

-- Browser reports refused because the GitHub account is not linked (spec 7).
-- Nothing here counts toward statistics; it exists so a fixable setup problem is
-- visible instead of silently dropping the report.
CREATE TABLE IF NOT EXISTS browser_reports_rejected (
    event_id        TEXT PRIMARY KEY NOT NULL,
    login           TEXT NOT NULL,
    repository_id   TEXT NOT NULL,
    -- The original payload, kept so the report can be replayed after linking.
    payload         TEXT NOT NULL,
    received_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rejected_login
    ON browser_reports_rejected (login, received_at);

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

-- ---------------------------------------------------- repeatable achievements
-- One row per trigger, not one row per achievement. The permanent table above is
-- deliberately untouched: mixing these lifecycles would let the first trigger
-- suppress all later ones. The UNIQUE key is the idempotency boundary; no caller
-- may rely on having checked an in-memory set before inserting.
CREATE TABLE IF NOT EXISTS repeatable_achievement_events (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    achievement_id      TEXT NOT NULL,
    triggering_event_id TEXT NOT NULL,
    unlocked_at         INTEGER NOT NULL,
    rule_version        INTEGER NOT NULL,
    -- The repository where the triggering commit happened, for the detail list.
    canonical_repository_id TEXT NOT NULL,
    FOREIGN KEY (triggering_event_id) REFERENCES events (event_id) ON DELETE RESTRICT,
    UNIQUE (achievement_id, triggering_event_id)
);

CREATE INDEX IF NOT EXISTS idx_repeatable_achievement_time
    ON repeatable_achievement_events (achievement_id, unlocked_at DESC, id DESC);

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
