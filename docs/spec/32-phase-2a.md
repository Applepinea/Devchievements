# 第 32 章　Phase 2a：本地 Git 到通知的真实闭环

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 32. Phase 2a：本地 Git 到通知的真实闭环

目标：完成可真实运行的最小本地产品，不追求界面完整。

## 32.1 实现内容

- Tauri Companion（最小形态）与基础系统托盘。
- Rust Helper；`post-commit`、`post-rewrite`、`post-merge`；Hook 安装、备份、链式执行、卸载与恢复。
- IPC 优先、Spool 回退；Companion 启动时消费 Spool。
- Git 仓库识别；Git 邮箱配置（最小设置界面）。
- SQLite + SQLx 持久化；Engine 调用；Outbox Consumer（Desktop 子集状态机）。
- 桌面弹窗（默认样式）、默认提示音、普通 Commit 轻提示音、每日等级升级通知、永久成就通知。
- 通知队列与去重；Companion 退出重启后的恢复。
- Privacy Mode 基础行为；最小诊断视图。

不在本阶段：Dashboard 与图鉴等完整页面、历史导入界面、Sound Manager、Asset Manager、Popup Editor。

## 32.2 必须完成的真实测试

创建新仓库并提交；连续多次提交；重复事件输入；Amend；Rebase；本地 Merge；Companion 在线提交；Companion 关闭时提交并在重启后经 Spool 恢复；Hook 原有配置不被破坏；Hook 失败不影响 Commit；残留 Hook 在缺少 Helper 时静默退出；数据库关闭重启；Outbox 未完成任务恢复；通知重复触发防护；多仓库统计；Spool 并发写入与损坏恢复。

## 32.3 必须演示的闭环

```text
Git Commit → Git Hook → Rust Helper → IPC（失败则 Spool）→ Companion
→ 验证与去重 → SQLite → Core Engine → 成就与每日等级判断
→ 事务写入 Outbox → Outbox Consumer → 提示音与桌面弹窗
```

Companion 离线时，必须通过 Spool 恢复同一流程，并应用新鲜度策略。

## 32.4 验收

真实 Commit、真实持久化、真实统计、真实成就与每日等级、真实通知与提示音；Spool 恢复与 Hook 卸载测试通过；测试报告完成；延迟基准已测量（说明设备、样本量与方法）。不得使用静态模拟数据冒充真实闭环。

## 32.5 Crash Recovery 测试矩阵

在以下时刻强制终止 Companion（或 Helper），重启后验证已持久化事件不丢失、不重复计数、不重复通知：

1. Hook 写入 Spool 的过程中。
2. IPC 消息已到达但确认返回之前。
3. 事务提交之前。
4. 事务提交之后、Outbox 消费之前。
5. Outbox 消费中。
6. 通知已展示、状态尚未写入 `completed` 之前（按「桌面展示的崩溃语义」处理）。

事务提交点必须位于 Engine 输出之后、任何外部副作用之前。

---
