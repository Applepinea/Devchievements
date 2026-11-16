# 第 6 章　仓库身份与去重

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 6. 仓库身份与去重

## 6.1 Canonical Repository Identity

优先级：1) GitHub Repository ID；2) 规范化 GitHub Owner/Repo；3) 规范化其他 Remote URL；4) 无 Remote 时生成并持久化本地 UUID。

必须支持：SSH / HTTPS Remote、`.git` 后缀、URL 大小写规范化、多个 Remote、Fork、Repository Rename、Git Worktree、Remote Alias。不得仅依赖本地文件夹名称，也不得轻率合并不同平台的同名仓库；仓库改名后尽量保留原有 canonical identity。

## 6.2 Commit 去重

主去重键：`canonical_repository_id + commit_sha`。Patch ID 仅作辅助，不能作为绝对去重依据（不同分支可能合法存在相同 Patch）。

Rebase、Amend、Cherry-pick、Squash 可能产生新 SHA，不能承诺跨所有 Git 操作实现绝对去重；对可识别的 Rewrite 关系必须进行关联处理，并明确记录局限与误判风险。

---
