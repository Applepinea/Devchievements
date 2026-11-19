# ADR-0020：新鲜度策略在摄取路径上生效，中间档走汇总而非弹窗

- **状态**：Accepted（2026-11-19 实现）
- **日期**：2026-11-19
- **相关规范章节**：§16.3（新鲜度策略）、§16.4（通知默认行为表）、§11.2（历史导入静默）、§13.4（解锁来源）
- **取代 / 被取代**：无

## 背景（Context）

§16.3 把事件按年龄分三档：0–10 分钟正常通知，10 分钟–24 小时汇总通知，超过 24 小时静默更新统计。
但在此之前，这条策略**只存在于 `devchievements-notifications` 和属性测试里，真正写数据库的路径从来不看年龄：

- `ingest_event_with_policy` 接受 `NotificationPolicy`，但该枚举只有 `Inform` / `Quiet` / `Silent` 三档，没有「汇总」这一档；
- `spool_consumer::ingest_record` 把 `FreshnessAction::Silent` 映射成 `Quiet`，而 `Digested` 掉进通配臂 `_ => Inform`，于是 10 分钟到 24 小时的事件**照样写一条待展示的 Outbox 行**；
- 结果：周末后重放 Spool（或 Companion 离线一段时间再启动）会弹出一堆「刚刚解锁」。这不是理论风险，是修复清单 T3 报告的实际现象。

同时有两处注释在撒谎：`spool_consumer.rs` 的模块文档说「silent after 24 hours, digested in between」，
而下方注释写「Digested 仍然给一条完整弹窗」，代码按后者执行。

## 决定（Decision）

1. **`NotificationPolicy` 增加 `Digest` 档**，且判断谓词从 `== Inform` 改成 `matches!`：
   `Inform`（每个意图一行）、`Digest`（整个事件一行汇总）、`Quiet` / `Silent`（一行不写）。新增档位不再被静默算作 Inform。
2. **年龄 → 策略的映射放在 `spool_consumer`，且不写通配臂**。
   `FullNotification → Inform`、`Digested → Digest`、`Silent → Quiet`。
   将来给 `FreshnessAction` 加新档位会编译失败，而不是掉进「全部弹窗」。
3. **静默档用 `Quiet` 而不是 `Silent`**：`Silent` 的含义是「历史导入」，会把 `unlock_source` 写成 `import`（§13.4）。
   事件确实发生在用户账号上，只是我们**知道得晚**，把它标成导入是图鉴里的一句谎话。
4. **汇总仍是 Outbox 行**（`kind = 'digest_summary'`），不是「不写行」。
   理由：§16.3 要求中间档有汇总可见，完全不写行等于静默；走 Outbox 也让它具备幂等 id、崩溃恢复与「attempt 先落盘」的既有保证。
   行内容只放 `events` 与 `unlocks`，不重复成就表。
5. **合并发生在投递时**（`OutboxProcessor::deliver_pending`），与 §16.6 的弹窗合并同一层：
   一次投递把所有 digest 行折成**一个** `Presentation::Digest`。存储仍然一个意图一行状态机，呈现负责「用户只看到一次」。
6. **`Presentation` 增加 `Digest(DigestSummary)` 变体**，桌面端与 headless 端各自实现：
   桌面写日志并把摘要放到托盘 tooltip（托盘是常驻的唯一可见表面），**不创建窗口、不播庆祝音效**。

## 备选方案（Alternatives）

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 中间档仍然弹窗（现状） | 零改动 | 直接违反 §16.3，周末后弹幕 | 否 |
| 中间档也完全不写行（当成 Quiet） | 改动最小 | 违反 §16.3「汇总通知」，用户永远看不到发生了多少事 | 否 |
| 汇总行沿用 `achievement_unlocked` kind + 特殊 payload | 不改协议 | 投递层要猜，kind 常量失真 | 否 |
| 每次投递都弹一条汇总（不合并） | 实现简单 | 50 条重放 = 50 条汇总，等于没解决 | 否 |
| 存储层累积全局汇总行（单行 upsert） | 天然只有一条 | 要新表或可变的行语义，破坏「一意图一状态机」 | 否 |

## 后果（Consequences）

- **正面**：§16.3 的三档真正生效；周末重放不再弹窗；注释与行为一致；`Digest` 与 `Inform` 的差别被单测与集成测试双向钉住。
- **负面 / 代价**：
  - `Presentation` 多一个变体，全部穷尽 match 都要更新（本次已全部更新）。
  - 桌面端的汇总只落在托盘 tooltip 与日志里，**没有做专门的汇总窗口**。这是已知的形态选择，不是遗漏：§16.3 只要求「汇总通知」，没有规定形态。
  - 汇总标题是硬编码英文（`DigestSummary::headline`），未接入 i18n。
- **引入的新风险**：`digest_summary` 是新的 Outbox kind，若将来有外部读者假设 kind 只有三种会被它绊到。已在写入处注释。
- **需要在后续 Phase 复查的点**：汇总是否应有自己的小窗；汇总文案本地化；§16.3 说「用户可以调整」分档阈值，目前阈值仍是常量，没有设置界面。

## 证据（Evidence）

- 修复前的失败测试（先写、先失败）：`crates/companion-core/tests/companion_loop.rs` 的 `the_freshness_bands_decide_what_the_user_sees` 在 11 分钟档失败：
  `band-11m: the digest band must not open a window, got [Popup(Popup { title: "First Commit", ... })]`。
- 修复后：`node scripts/cargo-run.mjs test -p devchievements-companion-core` → 20 passed；
  `-p devchievements-storage` → 47 passed；`-p devchievements-notifications` → 27 passed。
- 边界用例覆盖 9 分钟 / 11 分钟 / 23 小时 59 分 / 24 小时 1 分；50 条重放断言 0 个弹窗、至多 1 条汇总。
- `clippy --workspace --all-targets -- -D warnings` 与 `cargo fmt --all` 均通过。

## 复查计划（Review）

Phase 4b 结束时复查一次：若托盘 tooltip 被判定为不足以承载汇总，改为一条独立的轻量汇总窗，并同步更新本 ADR 的「代价」一节。
