# 第 8 章　Git 活动统计规则

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 8. Git 活动统计规则

## 8.1 默认规则

| 类型 | 默认处理 |
| --- | --- |
| 普通有效 Commit | 计入 |
| Empty Commit | 排除 |
| Bot Commit | 排除 |
| 非用户本人 Commit | 排除 |
| Merge Commit | 不进入普通 Commit 统计，可单独分类 |
| Cherry-pick | 默认计入 |
| Amend | 结合 Rewrite 信息关联，不重复计数 |
| Rebase | 结合 post-rewrite 映射处理 |
| Push | 不等于 Commit，不直接增加 Commit 数 |

## 8.2 GitHub 合并

GitHub 上的 PR Merge、Squash Merge、Rebase Merge 只计入 PR Merge，不额外计入普通 Commit。不得把一次 Push 中的多个 Commit 合并为一个 Commit 事件。

## 8.3 时间记录

Commit 保存 Author Timestamp、Committer Timestamp、Observed At、Local Date、Timezone。History Import 使用 Commit 时间归档；Spool 与 IPC 事件保留 Hook 实际触发时间，不得用 Companion 的消费时间替代。

---
