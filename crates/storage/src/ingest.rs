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
    // Spec 13.2 / 14.1 (checklist T1): both numbers the Engine judges are about the
    // USER, not about one working copy. Three commits in A plus two in B plus one in C
    // is a six-commit day, and Century means a hundred commits anywhere.
    //
    // The per-repository counters are still maintained and still read -- by the
    // Repositories page and by `statistics()` -- so nothing is lost by reading the
    // global figures here.
    let _ = repository_id;
    let repository_commit_count = projections::global_commit_total(pool).await?;

    let daily = projections::daily_global_projection(pool, local_date).await?;
    let (daily_commit_count, daily_rank) = match daily {
        Some(day) => (day.commit_count, day.daily_rank),
        None => (0, None),
    };

    let unlocked = projections::unlocked_achievement_ids(pool).await?;

    Ok(StateSnapshot {
        version: 0,
        repository_commit_count,
        daily_commit_count,
        daily_rank_today: daily_rank,
        unlocked_achievement_ids: unlocked,
        // Deliberately not loaded: a commit evaluation declares no browser counters
        // in its read-set, and filling them "just in case" is how a read-set drifts.
        ..Default::default()
    })
}

/// The browser activity an event type represents, if any.
///
/// An opened pull request, a closed issue and a received star are recorded but have
/// no counter, because no rule reads one. Mapping them to something invented would
/// mean every reader has to guess whether the number is maintained.
pub fn browser_activity_for(
    event_type: devchievements_events::EventType,
) -> Option<devchievements_core::BrowserActivity> {
    use devchievements_core::BrowserActivity;
    use devchievements_events::EventType;

    match event_type {
        EventType::PullRequestMerged => Some(BrowserActivity::PullRequestMerged),
        EventType::ReleasePublished => Some(BrowserActivity::ReleasePublished),
        EventType::RepositoryCreated => Some(BrowserActivity::RepositoryCreated),
        EventType::StarGiven => Some(BrowserActivity::StarGiven),
        _ => None,
    }
}

/// Load the read-set a browser activity declared (spec 4.3).
///
/// No day projection is read: browser activities do not move the daily rank
/// (spec 14.3 counts commits).
pub async fn load_browser_snapshot(
    pool: &SqlitePool,
    repository_id: &str,
) -> StorageResult<StateSnapshot> {
    let _ = repository_id;
    let snapshot = StateSnapshot {
        version: 0,
        // Global for the same reason: a merged pull request may satisfy a commit
        // achievement, and that achievement is about the user's total (spec 13.2).
        repository_commit_count: projections::global_commit_total(pool).await?,
        daily_commit_count: 0,
        daily_rank_today: None,
        unlocked_achievement_ids: projections::unlocked_achievement_ids(pool).await?,
        ..Default::default()
    };

    let mut snapshot = snapshot;
    projections::browser_totals(pool)
        .await?
        .apply_to(&mut snapshot);
    Ok(snapshot)
}

/// Whether one event may produce user-visible notifications.
///
/// Spec 16.3 splits events by age into three bands, and each band is a different
/// promise to the user: celebrate now, summarise later, or only move the numbers.
/// The policy is passed in rather than derived here because only the caller knows
/// how old the event is; keeping the clock out of storage also makes each band
/// testable without waiting.
///
/// Spec 11.2: a history import is silent. The achievements still unlock and are still
/// recorded with their source and rule version; only the outbox rows are suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationPolicy {
    /// Queue one celebration per intent (spec 16.4).
    Inform,
    /// Queue a single summary row instead of a celebration per intent (spec 16.3).
    ///
    /// Used for an event between ten minutes and a day old: the user was away, so a
    /// window per record would be the pile of "just unlocked" popups a Spool replay
    /// after a weekend used to produce.
    Digest,
    /// Record everything but queue nothing.
    ///
    /// Used for an event that is real but too old to announce (spec 16.3, over 24
    /// hours). It is NOT a history import: the unlock source stays realtime, because
    /// the event did just happen to the user's account.
    Quiet,
    /// Record everything but queue nothing (history import).
    Silent,
}

impl NotificationPolicy {
    /// Whether this policy may create one outbox row per intent.
    pub fn queues_notifications(self) -> bool {
        matches!(self, NotificationPolicy::Inform)
    }

    /// Whether this policy folds its intents into one summary row.
    pub fn digests_notifications(self) -> bool {
        matches!(self, NotificationPolicy::Digest)
    }

    /// Whether unlocks from this policy should be labelled as imported.
    pub fn is_import(self) -> bool {
        matches!(self, NotificationPolicy::Silent)
    }
}

/// Ingest a commit event.
///
/// The whole operation runs inside one SQLite transaction; any failure rolls back
/// to exactly the state before the call.
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
    ingest_event_with_policy(
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

/// Ingest one event with an explicit notification policy.
///
/// The policy has to be decided by the caller because only the caller knows how old
/// the event is (spec 16.3). Passing it in, rather than letting this layer re-derive
/// freshness, keeps the clock out of storage and makes the decision testable.
#[allow(clippy::too_many_arguments)]
pub async fn ingest_event_with_policy(
    pool: &SqlitePool,
    event: &Event,
    facts: &CommitFacts,
    now_ms: i64,
    identity: &Identity,
    ctx: &InjectedContext<'_>,
    policy: NotificationPolicy,
    user_rules: &[devchievements_core::achievements::definition::UserAchievement],
) -> StorageResult<IngestOutcome> {
    ingest_commit_inner(
        pool, event, facts, now_ms, identity, ctx, policy, user_rules,
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

    // BEGIN IMMEDIATE, not a deferred BEGIN. A deferred transaction only takes the
    // write lock at the first INSERT, so two concurrent deliveries of one event both
    // pass the duplicate check and one then fails with "database is locked" -- after
    // the user's commit has already been accepted by the spool. Taking the write lock
    // up front serialises the check-then-insert pair and lets the busy timeout wait
    // for the other writer instead of failing.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

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
    //
    // The insert itself is the final idempotency authority, not the SELECTs above.
    // Two concurrent deliveries can both pass a read, but only one can insert the
    // primary key (or the repository+sha unique index), so `rows_affected == 0` is
    // the race-free "already known" answer.
    // RETURNING is used instead of rows_affected() because SQLite reports the
    // statement's change count, and under contention that count proved able to report
    // an ignored insert as a change. A returned row exists only if THIS statement
    // actually inserted, which is the property the duplicate decision needs.
    let inserted = sqlx::query(
        "INSERT INTO events (event_id, event_type, source, source_confidence,
            canonical_repository_id, external_id, commit_sha, occurred_at, observed_at,
            received_at, local_date, timezone, date_policy, payload, schema_version,
            ingest_batch)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)
         ON CONFLICT DO NOTHING
         RETURNING event_id",
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
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        tx.rollback().await?;
        return Ok(IngestOutcome::Duplicate);
    }

    // ---- evaluate with the declared read-set ----
    //
    // Spec 2.3: a browser report is not a commit. It moves only its own counter and
    // is judged by the browser achievements; the dispatch is on the event type
    // because the Spool path carries both kinds through the same call.
    let activity = browser_activity_for(event.event_type);

    // A failed read is returned, never defaulted. Substituting an empty snapshot
    // would make the Engine judge this event against zero, persist that guess and
    // let the caller consume the Spool record: the commit would be lost, and the
    // only trace would be a statistic that silently stayed behind.
    let (snapshot, statistics) = match activity {
        Some(activity) => {
            let snapshot =
                load_browser_snapshot(pool, event.canonical_repository_id.as_str()).await?;
            let statistics = activity.statistics_with(&snapshot);
            (snapshot, statistics)
        }
        None => {
            let snapshot = load_snapshot(
                pool,
                event.canonical_repository_id.as_str(),
                event.local_date.as_str(),
            )
            .await?;
            let statistics = devchievements_core::achievements::StatisticsSnapshot {
                commit_count: snapshot.repository_commit_count.saturating_add(1),
                ..Default::default()
            };
            (snapshot, statistics)
        }
    };

    let output = match activity {
        Some(activity) => {
            Engine.evaluate_browser_event(&snapshot, activity, event.event_id.as_str(), ctx)
        }
        None => Engine.evaluate_commit(&snapshot, facts, event.event_id.as_str(), ctx),
    };

    // Spec 22: the user's own rules are judged against the same official statistics
    // the built-ins use, and inside the same transaction, so a rule can never unlock
    // without the event that caused it being recorded.
    let user_unlocks = if user_rules.is_empty() {
        Vec::new()
    } else {
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
                source: if policy.is_import() {
                    devchievements_core::UnlockSourceTag::Import
                } else {
                    devchievements_core::UnlockSourceTag::Realtime
                },
            },
        });
    }
    let mut queued = Vec::new();
    // Spec 16.3, the middle band: one summary for the whole event, never a window per
    // reward. The ids are collected while the unlocks are recorded and written as a
    // single row after the loop, so the digest cannot outrun the unlocks it describes.
    let mut digest_unlocks: Vec<String> = Vec::new();
    for intent in &intents {
        let (kind, payload) = match &intent.kind {
            SideEffectKind::CommitRecorded => ("commit_recorded", "{}".to_string()),
            SideEffectKind::DailyRankUpgrade { rank } => (
                "daily_rank_upgrade",
                format!("{{\"rank\":\"{}\"}}", rank.as_str()),
            ),
            SideEffectKind::RepeatableAchievementTriggered {
                achievement_id,
                triggering_event_id,
            } => {
                // The pair is a database-enforced idempotency key. The event's own
                // primary key and the commit-sha index stop a replay earlier, but this
                // constraint is deliberately independent of them: it remains correct
                // if a producer changes its event-id scheme or retries concurrently.
                let rule_version =
                    devchievements_core::achievements::builtin_repeatable_achievements()
                        .into_iter()
                        .find(|a| a.achievement_id == *achievement_id)
                        .map(|a| i64::from(a.rule_version))
                        .unwrap_or(1);
                let result = sqlx::query(
                    "INSERT OR IGNORE INTO repeatable_achievement_events
                        (achievement_id, triggering_event_id, unlocked_at, rule_version,
                         canonical_repository_id)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(achievement_id)
                .bind(triggering_event_id)
                .bind(now_ms)
                .bind(rule_version)
                .bind(event.canonical_repository_id.as_str())
                .execute(&mut *tx)
                .await?;

                // A retry of an already-recorded trigger may not manufacture a second
                // notification, even if the caller supplies a different outbox id.
                if result.rows_affected() == 0 {
                    continue;
                }
                let payload = serde_json::json!({
                    "achievement_id": achievement_id,
                    "triggering_event_id": triggering_event_id,
                })
                .to_string();
                ("achievement_triggered", payload)
            }
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

        // Spec 11.2 and 16.3: an import, or an event too old to announce, must not
        // queue a notification. The unlock itself is still recorded above, with its
        // source and rule version -- suppressing the row here is what makes the
        // policy real, because a row that exists gets delivered by the outbox.
        //
        // The digest band is the exception: it queues, but only as a summary. The
        // reward is remembered here and folded into one row after the loop.
        if policy.digests_notifications() {
            match &intent.kind {
                SideEffectKind::AchievementUnlocked { achievement_id, .. } => {
                    digest_unlocks.push(achievement_id.clone())
                }
                SideEffectKind::DailyRankUpgrade { rank } => {
                    digest_unlocks.push(format!("daily:{}", rank.as_str()));
                }
                // A plain commit is not a reward, so it never appears in a summary.
                SideEffectKind::CommitRecorded => {}
                // A repeatable completion is not an unlock summary: it happens on
                // ordinary commits, and folding it in would turn a quiet replay back
                // into a stream of "you were away" summaries (spec 16.3/16.4).
                SideEffectKind::RepeatableAchievementTriggered { .. } => {}
            }
            continue;
        }

        if !policy.queues_notifications() {
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

    if !digest_unlocks.is_empty() {
        let notification_id = format!("digest:{}", event.event_id.as_str());
        let payload = serde_json::json!({
            "events": 1,
            "unlocks": digest_unlocks,
        })
        .to_string();

        sqlx::query(
            "INSERT OR IGNORE INTO outbox
                (notification_id, event_id, status, created_at, kind, payload)
             VALUES (?, ?, 'created', ?, 'digest_summary', ?)",
        )
        .bind(&notification_id)
        .bind(event.event_id.as_str())
        .bind(now_ms)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;

        queued.push(notification_id);
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

/// Whether a table already has a column.
async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> StorageResult<bool> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .any(|row| row.get::<String, _>("name") == column))
}

/// The recorded schema version, treating "the table does not exist yet" as `None`.
///
/// A database that has never been migrated has no `schema_meta` table at all, so the
/// query fails. That is the fresh-install case, not an error worth propagating: it is
/// also the case where a backup would be a copy of nothing.
async fn schema_version_of_unmigrated(pool: &SqlitePool) -> Option<i64> {
    recorded_schema_version(pool).await.unwrap_or_default()
}

/// Write a pre-migration backup next to the database file.
///
/// # Why the path comes from the pool
///
/// The directory is taken from the database the pool is actually connected to, so a
/// caller cannot hand in the wrong one. That is also what makes it safe to do this
/// inside [`migrate`] with no extra parameter.
///
/// # Why a failure aborts the migration
///
/// A skipped backup turns "your data can be restored" into a claim nobody checked. If
/// the copy cannot be written the migration does not run, and the error says why.
async fn take_pre_migration_backup(pool: &SqlitePool, from_version: i64) -> StorageResult<()> {
    let filename = pool.connect_options().get_filename().to_path_buf();
    let directory = filename
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let backups = directory.join("backups");
    let path = backups.join(format!(
        "pre-migration-v{from_version}-to-v{}-{}.db",
        schema::SCHEMA_VERSION,
        now_millis_for_backup()
    ));

    crate::backup::backup(pool, &path).await?;
    println!("MIGRATION backup={}", path.display());
    Ok(())
}

/// Milliseconds since the epoch, for a backup filename.
///
/// Local to this function on purpose: the rest of the crate takes the clock from the
/// caller, but a filename is not business logic and threading a clock through the
/// migration signature to name a file would be worse than this one call.
fn now_millis_for_backup() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Apply the schema and record its version.
///
/// Older databases are upgraded in place: `CREATE TABLE IF NOT EXISTS` leaves an
/// existing table untouched, so a column added in a later version has to be applied
/// explicitly. The check is idempotent, which is what allows this to run on every
/// open rather than once.
pub async fn migrate(pool: &SqlitePool) -> StorageResult<()> {
    // Read BEFORE the schema is applied: this is the only way to tell a fresh database
    // from one an older build already filled, which decides whether a backup and the
    // global-projection backfill below are needed.
    let previous = schema_version_of_unmigrated(pool).await;

    // AGENTS.md 11 requires a backup before an irreversible migration, and this is the
    // ONE place every caller funnels through. Putting it here rather than at each of the
    // seven call sites is deliberate: a backup that has to be remembered at every call
    // site is a backup that will be missing at the eighth.
    //
    // The data directory is derived from the pool's own filename, so a caller cannot
    // pass a mismatched directory and write the backup somewhere unrelated.
    if previous.is_some_and(|version| version < schema::SCHEMA_VERSION) {
        take_pre_migration_backup(pool, previous.unwrap_or(0)).await?;
    }

    sqlx::raw_sql(schema::SCHEMA_SQL).execute(pool).await?;

    for (table, column, definition) in schema::COLUMN_UPGRADES {
        if !column_exists(pool, table, column).await? {
            sqlx::raw_sql(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))
            .execute(pool)
            .await?;
        }
    }

    // Schema 4 added `daily_global_projection`. An existing database has per-repository
    // day rows describing exactly the same commits, so the global rows can be derived
    // from them instead of forcing a full event replay: this is a pure aggregation of
    // data that is already correct, so it cannot lose an event or revoke an unlock.
    if previous.is_some_and(|v| v < 4) {
        backfill_global_days(pool).await?;
    }

    sqlx::query(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(schema::SCHEMA_VERSION.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Sum the existing per-repository day rows into the global day table.
///
/// Idempotent: it rewrites every global row from scratch, so running it twice or on a
/// half-migrated database converges on the same numbers. The rank is recomputed from
/// the summed count rather than copied, because a day that was Common per repository
/// can be Rare once the repositories are added together -- which is the whole point.
async fn backfill_global_days(pool: &SqlitePool) -> StorageResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM daily_global_projection")
        .execute(&mut *tx)
        .await?;

    let rows = sqlx::query(
        "SELECT local_date,
                SUM(commit_count) AS commits,
                MAX(updated_at)   AS updated_at,
                MAX(timezone)     AS timezone,
                MAX(date_policy)  AS date_policy
         FROM daily_projection
         GROUP BY local_date",
    )
    .fetch_all(&mut *tx)
    .await?;

    let thresholds = devchievements_daily_rank::DailyRankThresholds::official();
    for row in rows {
        let date: String = row.get("local_date");
        let commits: i64 = row.get("commits");
        let rank = thresholds.rank_for(commits as u32);
        sqlx::query(
            "INSERT INTO daily_global_projection
                (local_date, commit_count, daily_rank, timezone, date_policy, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&date)
        .bind(commits)
        .bind(rank.map(|r| r.as_str()))
        .bind(row.get::<String, _>("timezone"))
        .bind(row.get::<i64, _>("date_policy"))
        .bind(row.get::<i64, _>("updated_at"))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
