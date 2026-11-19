# ADR-0022：可重复成就使用独立触发表，不混入永久成就与每日等级

- **状态**：Accepted（2026-11-20 实现）
- **日期**：2026-11-20
- **相关规范章节**：§13.1（永久成就只解锁一次）、§13.2（成就按用户）、§16.4（普通提交只播提示音）、§16.6（同一事件合并为一个弹窗）、§14.1（每日等级独立）、AGENTS.md §11（不可逆迁移需备份）
- **取代 / 被取代**：无
- **影响**：schema 4 → 5，新增表 `repeatable_achievement_events`

## 背景（Context）

规范把「成就」写成一次性解锁，因此 `unlocked_achievements` 的主键是 `achievement_id`。本任务要求新增一个**可重复**成就 `commit_complete`：每次有效 Commit 触发一次，同一个事件重复处理只能记一次，不同事件可以各记一次。

直接复用永久表不可行：第一次触发之后，主键会让后续触发全部被 `INSERT OR IGNORE` 吞掉。复用 `daily_milestones` 也不可行：它的键是 `(milestone_id, local_date)`，同一天多次提交会被压成一次。

`crates/achievements` 里的 `evaluate()` 会跳过 `existing_unlocks`，这正是永久成就需要的规则，也正好是可重复成就**不能**使用的规则。仅把 `repeatable` 字段设为 true 不会改变任何行为。

## 决定（Decision）

1. **两套目录、两条生命周期。** 9 个永久成就继续由 `builtin_achievements()` 提供，ID、条件、稀有度、隐藏属性都不变；`commit_complete` 由独立的 `builtin_repeatable_achievements()` 提供，不进入图鉴的「永久」目录。
2. **新增 `repeatable_achievement_events` 表。** 字段为 `id`、`achievement_id`、`triggering_event_id`、`unlocked_at`、`rule_version`、`canonical_repository_id`，并声明
   `UNIQUE (achievement_id, triggering_event_id)` 与外键到 `events(event_id)`。
   幂等性由数据库约束和事务保证，不依赖内存集合。
3. **Engine 为每个被计数的 Commit 产生一个独立意图。**
   `evaluate_repeatable()` 不读取已解锁集合，只判断当前规则与 Capability；`evaluate_commit` 在 empty / merge 过滤之后调用它，并把 `event_id` 一并带进 `SideEffectKind::RepeatableAchievementTriggered`。通知 ID 使用 `ach:commit_complete:<event_id>`，与永久成就共用同一套 ID 语法，但每个事件各不相同。
4. **存储层在同一事务内写入触发记录与 Outbox。**
   只有本次真正插入了 `(achievement, event)` 才继续写通知；插入冲突时不制造第二条通知。事件行本身也以 `INSERT ... ON CONFLICT DO NOTHING RETURNING` 作为最终幂等判据，而不是只依赖前置 SELECT。
5. **展示仍遵守 §16.4。** 可重复触发的 Outbox 行有独立 ID 和独立记录，但 `presentation_for()` 不把它变成弹窗；普通提交仍然只播轻提示音。永久成就或每日等级升级时，弹窗照常合并，主标题仍取最高稀有度。
6. **汇总档不折叠可重复触发。** §16.3 的中间档只汇总真正的解锁；把每次提交都算成「你不在时解锁了成就」会让周末重放重新变成一串摘要。
7. **每日等级完全不动。** 等级仍按当天跨仓库有效提交数计算，存 `daily_projection` / `daily_global_projection` / `daily_milestones`，不写 `unlocked_achievements`，也不写触发表。
8. **迁移只加表，不补历史。** schema 5 由 `CREATE TABLE IF NOT EXISTS` 建立；v1–v4 升级前照旧 `VACUUM INTO` 备份。既有永久解锁原样保留，旧事件不会因为迁移被重放，也不会被补成触发记录。

## 备选方案（Alternatives）

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 在 `unlocked_achievements` 上放宽主键 | 少一张表 | 永久成就的唯一性被破坏，旧数据无法表达「哪次事件」 | 否 |
| 用 `daily_milestones` 存每天次数 | 复用现有表 | 同一天多次提交只留一条，语义错误 | 否 |
| 只在内存中去重 | 改动最少 | 重启、并发、重放全部失效；任务明确禁止 | 否 |
| 每次触发都弹窗 | 可见性最强 | 违反 §16.4，用户每次提交都会被打断 | 否 |
| 独立表 + 数据库唯一键 + 同一事务 | 与两套既有机制都兼容 | 多一张表和一次查询 | **采用** |

## 后果（Consequences）

- 新增查询 `read_models::repeatable_achievements` 与命令 `get_repeatable_achievements`；图鉴分「永久成就 / 可重复成就 / 每日等级」三块，可重复卡显示累计次数与逐条触发历史。
- `statistics().achievements` 与「未读」计数仍只统计永久表，可重复次数不会污染永久成就数。
- 历史导入（`Silent`）仍会写触发记录，但不写 Outbox；这是「导入静默但事实完整」的延续。
- **未做**：`streak_7/30/365` 仍因快照没有连续天数字段而不可达；每日等级的自定义阈值只有 crate 层能力，设置文件与 Engine 尚未接线；浏览器 IndexedDB 侧未同步这张表（当前 Browser Only 不写权威统计，见 ADR-0018）；触发行没有单独的「已读」标记。

## 证据

- `crates/storage/src/tests.rs`：`a_repeatable_achievement_records_every_commit_independently`、`replaying_one_commit_cannot_duplicate_its_repeatable_record`、`two_concurrent_deliveries_of_one_event_record_it_once`、`an_imported_commit_records_its_trigger_without_a_notification`、`upgrading_v4_to_v5_keeps_the_permanent_unlocks_and_adds_the_trigger_table`
- `crates/companion-core/tests/companion_loop.rs`：`every_repeatable_trigger_is_recorded_while_commits_stay_sound_only`、`a_digested_commit_with_no_permanent_reward_stays_completely_silent`
- `scripts/e2e/cross-repository.mjs`：真实 Hook → Spool → Companion → SQLite，六个提交对应六条独立触发记录与六条独立通知行。
