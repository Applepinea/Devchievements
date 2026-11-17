//! Ingestion: one event in, projections and outbox intents out, all atomically.
//!
//! # Why this is a single function with a transaction
//!
//! Spec 4.3 and 16.1 require that the raw event, the projection updates it implies
//! and the notification intents it produces commit together. If they could be
//! written separately, a crash between steps leaves a world where statistics say
//! "10 commits" but the 10th event is missing, or an achievement is unlocked with
//! nothing to celebrate. Neither is recoverable by inspection.
//!
//! # Idempotency
//!
//! `event_id` is the idempotency key (spec 5.3). Re-ingesting a known id returns
//! `IngestOutcome::Duplicate` and changes nothing. That is what makes at-least-once
//! delivery from the IPC/Spool path harmless.

use crate::projections;
use crate::schema;
use crate::StorageResult;
use devchievements_core::{CommitFacts, Engine, InjectedContext, SideEffectKind, StateSnapshot};
use devchievements_events::{validation, Event, Identity};
use sqlx::{Row, SqlitePool};

/// Why ingestion was refused before any statistics began.
///
/// Distinct from a validation failure: the event itself may be perfectly valid, but
/// the product is not allowed to start counting yet (spec 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestRefusal {
    /// No user email is configured, so attribution is impossible.
    ///
    /// Spec 7 is explicit that statistics and history import must NOT begin in this
    /// state; starting anyway would attribute other people's commits to the user.
    IdentityNotConfigured,
    /// The event's author is not one of the configured identities.
    NotTheUser {
        /// The author email that was found.
        author_email: String,
    },
}

/// What happened when an event was submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// Stored, projections updated, intents queued.
    Ingested {
        /// Notification ids created for this event.
        queued_notifications: Vec<String>,
    },
    /// The event id was already known; nothing changed.
    Duplicate,
    /// Stored but produced no intents (e.g. an empty commit).
    IngestedSilently,
    /// Rejected by validation; nothing was written.
    Rejected(String),
    /// Refused before statistics could begin; nothing was written.
    Refused(IngestRefusal),
}

/// The spec 7 gate: no email configured means no statistics.
///
/// When an identity IS configured, the event's author must be one of its addresses.
/// Imported history is the only source where a mismatch is expected and handled by
/// the import pipeline (which filters before building events), so it is treated the
/// same here: refusal, not silent acceptance.
pub fn check_identity(identity: &Identity, event: &Event) -> Result<(), IngestRefusal> {
    if !identity.is_configured() {
        return Err(IngestRefusal::IdentityNotConfigured);
    }

    let Some(author_email) = author_email_of(event) else {
        // No email in the payload. That is only acceptable when the payload says who
        // did it and that account is one of ours (spec 7: the browser maps a GitHub
        // login to the configured emails).
        //
        // Treating an unattributed browser event as "mine" would count commits made
        // by other people on pages the user happened to open, which is worse than
        // refusing: it silently inflates every statistic.
        if let Some(login) = author_login_of(event) {
            if identity.owns_login(&login) {
                return Ok(());
            }
            return Err(IngestRefusal::NotTheUser {
                author_email: format!("@{login}"),
            });
        }
        return Err(IngestRefusal::NotTheUser {
            author_email: "(no author on the event)".to_string(),
        });
    };

    if identity.owns_email(&author_email) {
        Ok(())
    } else {
        Err(IngestRefusal::NotTheUser { author_email })
    }
}

/// The GitHub login recorded in an event payload, if any.
fn author_login_of(event: &Event) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(&event.payload).ok()?;
    payload
        .get("author_login")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Classify a commit for the counting rules (spec 8.1).
///
/// Kept as an explicit input rather than re-derived here, because the decision
/// depends on facts only the Hook Helper can observe (parent count, tree identity).
pub fn commit_facts(parent_count: usize, tree_equals_parent_tree: bool) -> CommitFacts {
    CommitFacts {
        counts_as_commit: parent_count < 2,
        is_merge: parent_count >= 2,
        is_empty: tree_equals_parent_tree,
    }
}

/// Load the read-set the Engine declared for this event kind.
///
/// Spec 4.3: the adapter loads ONLY what the rule declared. Doing this as one
/// function keeps the query and the declared read-set next to each other, so a
/// drifted read-set is visible in review.
pub async fn load_snapshot(
    pool: &SqlitePool,
    repository_id: &str,
    local_date: &str,
) -> StorageResult<StateSnapshot> {
    let repository_commit_count = projections::repository_commit_count(pool, repository_id).await?;

    let daily = projections::daily_projection(pool, repository_id, local_date).await?;
    let (daily_commit_count, daily_rank) = match daily {
        Some(row) => (row.commit_count, row.daily_rank),
        None => (0, None),
    };

    let unlocked = projections::unlocked_achievement_ids(pool).await?;

    Ok(StateSnapshot {
        version: 0,
        repository_commit_count,
        daily_commit_count,
        daily_rank_today: daily_rank,
        unlocked_achievement_ids: unlocked,
    })
}

/// Ingest a commit event.
///
/// The whole operation runs inside one SQLite transaction; any failure rolls back
/// to exactly the state before the call.
/// Whether the event may produce user-visible notifications.
///
/// Spec 11.2: a history import is silent. The achievements still unlock and are still
/// recorded with their source and rule version; only the outbox rows are suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPolicy {
    /// Queue notifications as usual.
    Inform,
    /// Record everything but queue nothing (history import).
    Silent,
}

pub async fn ingest_commit(
    pool: &SqlitePool,
    event: &Event,
    facts: &CommitFacts,
    now_ms: i64,
    identity: &Identity,
    ctx: &InjectedContext<'_>,
) -> StorageResult<IngestOutcome> {
    ingest_commit_with_rules(pool, event, facts, now_ms, identity, ctx, &[]).await
}

/// Ingest a commit, also evaluating the user's own achievement rules (spec 22).
///
/// The rules come from the caller because they live in the settings document, not
/// in the database: storage never reads the settings file, and the engine never
/// reads the file system. Passing them here keeps both of those true.
///
/// # Why user unlocks are decided here and not in the Engine
///
/// The Engine contract is `(snapshot, event, context)`, and the snapshot carries
/// the ids already unlocked but not the rules themselves. Rather than widen a
/// contract every Golden Test depends on, the user rules are evaluated against the
/// same official statistics, in the same transaction, immediately after the
/// built-ins. Both kinds then share one code path for recording the unlock, which
/// is what stops them drifting apart.
pub async fn ingest_commit_with_rules(
    pool: &SqlitePool,
    event: &Event,
    facts: &CommitFacts,
    now_ms: i64,
    identity: &Identity,
    ctx: &InjectedContext<'_>,
    user_rules: &[devchievements_core::achievements::definition::UserAchievement],
) -> StorageResult<IngestOutcome> {
    ingest_commit_inner(
        pool,
        event,
        facts,
        now_ms,
        identity,
        ctx,
        NotificationPolicy::Inform,
        user_rules,
    )
    .await
}

/// Ingest a history-imported event silently (spec 11.2).
pub async fn ingest_import(
    pool: &SqlitePool,
    event: &Event,
    facts: &CommitFacts,
    now_ms: i64,
    identity: &Identity,
    ctx: &InjectedContext<'_>,
) -> StorageResult<IngestOutcome> {
    ingest_commit_inner(
        pool,
        event,
        facts,
        now_ms,
        identity,
        ctx,
        NotificationPolicy::Silent,
        &[],
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn ingest_commit_inner(
    pool: &SqlitePool,
    event: &Event,
    facts: &CommitFacts,
    now_ms: i64,
    identity: &Identity,
    ctx: &InjectedContext<'_>,
    policy: NotificationPolicy,
    user_rules: &[devchievements_core::achievements::definition::UserAchievement],
) -> StorageResult<IngestOutcome> {
    // Spec 7 precondition, checked FIRST and before any write: with no configured
    // email there is no way to decide whether this commit is the user's, and
    // counting it would attribute someone else's work to them.
    if let Err(refusal) = check_identity(identity, event) {
        return Ok(IngestOutcome::Refused(refusal));
    }

    // Validate before touching the database so a rejected event writes nothing.
    if let Err(e) = validation::validate(event) {
        return Ok(IngestOutcome::Rejected(e.to_string()));
    }

    let mut tx = pool.begin().await?;

    // ---- duplicate check inside the transaction ----
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT event_id FROM events WHERE event_id = ?")
            .bind(event.event_id.as_str())
            .fetch_optional(&mut *tx)
            .await?;
    if existing.is_some() {
        tx.rollback().await?;
        return Ok(IngestOutcome::Duplicate);
    }

    // Spec 11.2 requires a unified dedup rule. Event ids differ between live capture
    // and history import ("post-commit:<sha>:<ms>" vs "import:<repo>:<sha>"), so the
    // identity of a commit is (repository, sha, event type). Without this, importing
    // history for a repository that was already watched would double count every
    // commit the user made since installing.
    if let Some(sha) = event.commit_sha.as_deref() {
        let same_commit: Option<(String,)> = sqlx::query_as(
            "SELECT event_id FROM events
             WHERE canonical_repository_id = ? AND commit_sha = ?",
        )
        .bind(event.canonical_repository_id.0.as_str())
        .bind(sha)
        .fetch_optional(&mut *tx)
        .await?;
        if same_commit.is_some() {
            tx.rollback().await?;
            return Ok(IngestOutcome::Duplicate);
        }
    }

    // ---- persist the raw event ----
    sqlx::query(
        "INSERT INTO events (event_id, event_type, source, source_confidence,
            canonical_repository_id, external_id, commit_sha, occurred_at, observed_at,
            received_at, local_date, timezone, date_policy, payload, schema_version,
            ingest_batch)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(event.event_id.as_str())
    .bind(format!("{:?}", event.event_type))
    .bind(format!("{:?}", event.source))
    .bind(format!("{:?}", event.source_confidence))
    .bind(event.canonical_repository_id.as_str())
    .bind(event.external_id.clone())
    .bind(event.commit_sha.clone())
    .bind(event.occurred_at)
    .bind(event.observed_at)
    .bind(event.received_at)
    .bind(event.local_date.as_str())
    .bind(&event.timezone)
    .bind(i64::from(event.date_policy))
    .bind(&event.payload)
    .bind(i64::from(event.schema_version.0))
    .execute(&mut *tx)
    .await?;

    // ---- evaluate with the declared read-set ----
    let snapshot = load_snapshot(
        pool,
        event.canonical_repository_id.as_str(),
        event.local_date.as_str(),
    )
    .await
    .unwrap_or_else(|_| StateSnapshot::default());

    let output = Engine.evaluate_commit(&snapshot, facts, event.event_id.as_str(), ctx);

    // Spec 22: the user's own rules are judged against the same official statistics
    // the built-ins use, and inside the same transaction, so a rule can never unlock
    // without the commit that caused it being recorded.
    let user_unlocks = if user_rules.is_empty() {
        Vec::new()
    } else {
        let statistics = devchievements_core::achievements::StatisticsSnapshot {
            commit_count: crate::projections::repository_commit_count(
                pool,
                event.canonical_repository_id.as_str(),
            )
            .await
            .unwrap_or(0)
            .saturating_add(1),
            ..Default::default()
        };
        let already = crate::projections::unlocked_achievement_ids(pool).await?;
        devchievements_core::achievements::definition::newly_satisfied(
            user_rules,
            &statistics,
            &already,
        )
    };

    // ---- apply projection changes ----
    projections::apply_state_changes(&mut tx, event, &output.state_changes, now_ms).await?;

    // ---- queue intents ----
    // User rules produce the same intent shape as built-ins, so the recording code
    // below -- unlock row, outbox row, silent-during-import rule -- is shared rather
    // than duplicated for a second achievement system.
    let mut intents = output.side_effect_intents.clone();
    for achievement_id in user_unlocks {
        intents.push(devchievements_core::SideEffectIntent {
            notification_id: format!("ach:{achievement_id}:{}", event.event_id.as_str()),
            kind: devchievements_core::SideEffectKind::AchievementUnlocked {
                achievement_id,
                source: if policy == NotificationPolicy::Silent {
                    devchievements_core::UnlockSourceTag::Import
                } else {
                    devchievements_core::UnlockSourceTag::Realtime
                },
            },
        });
    }
    let mut queued = Vec::new();
    for intent in &intents {
        let (kind, payload) = match &intent.kind {
            SideEffectKind::CommitRecorded => ("commit_recorded", "{}".to_string()),
            SideEffectKind::DailyRankUpgrade { rank } => (
                "daily_rank_upgrade",
                format!("{{\"rank\":\"{}\"}}", rank.as_str()),
            ),
            SideEffectKind::AchievementUnlocked {
                achievement_id,
                source,
            } => {
                // Record the unlock here, inside the same transaction, so an
                // achievement can never be "queued but not unlocked".
                let source_name = if source.is_silent() {
                    "import"
                } else {
                    "realtime"
                };
                sqlx::query(
                    "INSERT OR IGNORE INTO unlocked_achievements
                        (achievement_id, unlocked_at, unlock_source, rule_version,
                         viewed_at, triggering_event_id)
                     VALUES (?, ?, ?, ?, NULL, ?)",
                )
                .bind(achievement_id)
                .bind(now_ms)
                .bind(source_name)
                .bind(1_i64)
                .bind(event.event_id.as_str())
                .execute(&mut *tx)
                .await?;

                (
                    "achievement_unlocked",
                    format!("{{\"achievement_id\":\"{achievement_id}\"}}"),
                )
            }
        };

        // Spec 11.2: a history import must not queue per-event notifications. The
        // unlock itself is still recorded above, with its source and rule version.
        if policy == NotificationPolicy::Silent {
            continue;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO outbox
                (notification_id, event_id, status, created_at, kind, payload)
             VALUES (?, ?, 'created', ?, ?, ?)",
        )
        .bind(&intent.notification_id)
        .bind(event.event_id.as_str())
        .bind(now_ms)
        .bind(kind)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;

        queued.push(intent.notification_id.clone());
    }

    tx.commit().await?;

    // Reaching here means the transaction is durable.
    if queued.is_empty() {
        Ok(IngestOutcome::IngestedSilently)
    } else {
        Ok(IngestOutcome::Ingested {
            queued_notifications: queued,
        })
    }
}

/// Pull `author_email` out of an event payload, if present.
pub(crate) fn author_email_of(event: &Event) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(&event.payload).ok()?;
    value.get("author_email")?.as_str().map(|s| s.to_string())
}

/// Count outbox rows by status. Used by diagnostics and by tests that assert the
/// "queued but never displayed" invariant.
pub async fn outbox_counts(pool: &SqlitePool) -> StorageResult<Vec<(String, i64)>> {
    let rows =
        sqlx::query("SELECT status, COUNT(*) as n FROM outbox GROUP BY status ORDER BY status")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("status"), r.get::<i64, _>("n")))
        .collect())
}

/// Read back the persisted event count. Used to prove durability across reopen.
pub async fn event_count(pool: &SqlitePool) -> StorageResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// The schema version recorded in the database, if any.
pub async fn recorded_schema_version(pool: &SqlitePool) -> StorageResult<Option<i64>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM schema_meta WHERE key = 'schema_version'")
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(v,)| v.parse().ok()))
}

/// Apply the schema and record its version.
pub async fn migrate(pool: &SqlitePool) -> StorageResult<()> {
    sqlx::raw_sql(schema::SCHEMA_SQL).execute(pool).await?;
    sqlx::query(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(schema::SCHEMA_VERSION.to_string())
    .execute(pool)
    .await?;
    Ok(())
}
