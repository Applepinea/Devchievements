# ADR-0021：每日等级与累计成就按跨仓库合计，新增全局每日投影

- **状态**：Accepted（2026-11-19 实现）
- **日期**：2026-11-19
- **相关规范章节**：§13.2（成就按用户）、§14.1（每日等级）、§14.3（每日只升一次）、§13.4（解锁不撤销）、AGENTS.md §11（不可逆迁移需确认 + 备份）
- **取代 / 被取代**：无
- **影响**：schema 3 → 4，新增表 `daily_global_projection`

## 问题

§14.1 的每日等级是**用户的今天**，§13.2 的累计成就是**用户的全部**。但实现里两者都读单仓库值：

- `daily_projection` 的主键是 `(canonical_repository_id, local_date)`；
- `ingest::load_snapshot` 用 `repository_commit_count(pool, repository_id)` 与单仓库的当日计数；
- Engine 拿到的 `StateSnapshot` 里那两个字段，语义上是「本仓库」。

后果：同一用户在 A 仓库提交 3 次、B 仓库 2 次、C 仓库 1 次，规范说今天是 **6 次（Rare）**，
实现看到的是三次各自独立的 3 / 2 / 1（最高才 Uncommon）。Century 也一样：
100 次提交分散在三个仓库永远不会解锁。

同时仪表盘的总提交是跨仓库求和，`read_models::statistics` 返回的是全局，` 于是**页面上的数字与成就进度基数不一致**。

## 决定

1. **新增 `daily_global_projection` 表**，主键只有 `local_date`。
   不用「在 `daily_projection` 里放一行 `repository_id = ''` 的哨兵行」：
   那会让每个读取方都得猜某个 id 是不是真的，仓库页也要额外过滤。
2. **Engine 读全局，仓库级计数照旧维护**。
   `load_snapshot` 改为读 `global_commit_total()` 与 `daily_global_projection()`；
   `repository_projection.commit_count` 仍然是「这个仓库有多少提交」，仓库页读它。
3. **状态变更名拆开**：Engine 发 `global_commit_total`（绝对值）与 `daily_commit_count`；
   存储层用 `+1` 维护 `repository_projection.commit_count`，用 `+1` 维护全局日。
   **这是实现过程中发现的真实缺陷**：最初把 Engine 的全局绝对值直接写进 `repository_projection.commit_count`，
   于是 `SUM(commit_count)` 把同一批提交按写入次数重复累加（测出 68920 而正确答案是 120）。
4. **迁移是加表 + 回填，不是重建**：v1–v3 库的 `daily_projection` 行本来就正确，
   全局行可以由它们按 `local_date` 求和得到，等级**重新按合计计算**
   （这正是「3 + 2 = 5 是 Rare」的来源，而不是照抄其中一行的 Uncommon）。
   纯聚合，不重放事件，所以不可能丢事件、不可能撤销解锁。
5. **迁移前自动备份**：`ingest::migrate_with_backup` 在任何 schema 变更前 VACUUM INTO 一份到
   `<data>/backups/pre-migration-v{N}-to-v{M}-{ms}.db`；备份失败则**中止迁移**，不跳过。
6. **重建也要覆盖全局表**：`rebuild_from_events` 同时重算 `daily_global_projection`，
   否则「重建」和「实时摄取」会给出两套数字。

## 备选方案

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 维持单仓库口径 | 零改动 | 直接违反 §13.2 / §14.1，两数不一致 | 否 |
| 只改 Engine，读时 SUM 聚合 | 无 schema 变更 | 每次提交都要扫全表，违背 §4.3 的 O(1) 读 | 否 |
| 全局行用哨兵 `repository_id` | 复用现有表 | 每个读取方要过滤，仓库页会列出假仓库 | 否 |
| 迁移时 `rebuild_from_events` 全量重放 | 语义最纯 | 大库耗时长，且重放要重算解锁 —— 一旦有 bug 就会撤销成就 | 否 |
| 加表 + 从既有投影回填（采纳） | 快、可重复、不可能撤销解锁 | 依赖 「既有 per-repo 行是正确的」这一前提 | 是 |

## 代价

- schema 3 → 4；旧库打开时多一条 `CREATE TABLE` + 一次按日期的 GROUP BY。
- 每次提交多两次写：一条 `repository_projection` 的 `+1`，一条全局日的 upsert。都在同一事务内。
- `StateSnapshot.repository_commit_count` 的**语义变了**（从仓库值变用户值）而字段名没变。
  这是刻意的：改名会波及 Engine、WASM 探测、Golden Tests 与所有读集断言，
  而语义写在 `engine.rs` 的字段注释与本节里。
- 备份目录会随版本升级累积；目前没有清理策略。

## 没有做的事

- **没有改连续天数**（`streak_*` 仍不可达）：那是 T2 的范围，需要先有全局每日投影再算 streak，
  本次只把地基打好。
- **没有改浏览器活动的口径**：PR / Release / Repository / Star 本来就是跨仓库 SUM（ADR-0019）。
- **没有提供「只看某个仓库」的成就模式**：规范没有这个需求，加它等于发明新口径。

## 证据

- 修复前的失败测试（先写、先失败）：`the_daily_rank_counts_every_repository_together` /
  `the_commit_total_counts_every_repository_together` 编译失败（`cannot find function `daily_global_projection``）。
- 修复中发现的真实缺陷：把全局值写进 per-repo 列后，`the_commit_total_counts_every_repository_together` 断言 120 实测 **68920**。
- 迁移测试：`upgrading_an_old_database_backs_up_first_and_keeps_every_unlock` ——
  手工构造 v3 库（A 3 次 + B 2 次，两行都是 Uncommon，已有 `first_commit` 解锁），
  断言备份文件存在且仍是 v3、迁移后全局日 = 5 且等级 = **Rare**、解锁仍在。
- 真二进制端到端：`node scripts/e2e/cross-repository.mjs` → 9 项 PASS，
  三个真实仓库 3/2/1 次提交经真 hook → Spool → headless Companion，
  断言全局日 = 6、等级 = Rare、升级顺序为 common → uncommon → rare、仪表盘总数 == 成就基数。
- `node scripts/cargo-run.mjs test --workspace` → **481 passed, 0 failed**。

## 复查计划

T2 做连续天数时复查：streak 需要按 `daily_global_projection` 的 `local_date` 排序计算，
若那时发现本表结构不足以支撑（例如缺少「当日是否有效」的判定），在此 ADR 追加修订节。
