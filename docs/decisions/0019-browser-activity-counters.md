# ADR-0019：浏览器活动用独立计数列，成就读用户汇总

- **状态**：Accepted（2026-11-18 实现，无需产品决策，属实现细节）
- **影响**：`repository_projection` 增加 5 列，schema 版本 1 → 2

## 问题

浏览器报告的 PR Merge、Release、Issue 等事件此前**只入库、不计数**：
Engine 只有 `evaluate_commit`，所有记录都走提交路径（由此产生过「合并的 PR
被计为一次提交并解锁 First Commit」）。修掉误计之后，这些事件变成「存下来了，
但什么也不会发生」——成就永远不解锁，用户看不到任何反馈。

而成就规则里本来就有 `PullRequestsMergedAtLeast`、`ReleasesAtLeast`、
`RepositoriesCreatedAtLeast`、`StarsGivenAtLeast`，它们需要一个可读的计数。

## 决定

1. **每个仓库一列**，而不是一行全局汇总：活动发生在某个仓库里，
   「这个数字是哪来的」必须能回答。
2. **成就读 SUM**：规则讲的是「你」而不是「这个仓库」，
   所以 `StateSnapshot` 里放的是跨仓库合计。
3. **只为有一条规则在读的活动建列**：PR Merge、Release、Repository Created、
   Star Given。PR Opened / Issue Closed 仍然只入库——建一个没人读的计数器，
   会让每个读者都得猜它是否被维护。
4. **迁移是加列**：`ingest::migrate` 用 `PRAGMA table_info` 判断列是否存在，
   缺则 `ALTER TABLE ADD COLUMN`。投影是派生数据，加列不丢任何事件。

## 代价

- 已有数据库要跑一次 5 条 `ALTER TABLE`（幂等，每次打开都检查，代价是 5 次 PRAGMA）。
- 投影重建（`rebuild_from_events`）多 5 条 UPDATE 子查询；它本来就要重算，
  只是从「只算提交」变成「提交 + 浏览器活动」。
- 浏览器活动**不移动每日等级**（§14.3 只数提交）。这条写进了 Engine 的测试，
  因为「PR 合并算不算今天的活跃」很容易被后来的人想当然地加上。

## 没有做的事

- 浏览器活动本身不弹窗（§16.4 的通知默认行为表里没有这一行）。
  它靠**解锁成就**产生弹窗，这与「普通 Commit 只给轻提示音」是同一套逻辑。
- 「用户所有仓库的提交总数」仍按仓库统计（既有行为），本次没有改动；
  跨仓库口径的问题记录在 `docs/development/phase-4b-progress.md`。
