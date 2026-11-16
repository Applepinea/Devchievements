# 第 47 章　最终交付验收清单

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 47. 最终交付验收清单

**Core**

- [ ] Engine、事件系统、统计投影。
- [ ] 仓库身份与用户身份识别、去重、时区。
- [ ] Golden Tests 与 Hook trace 回放。

**Git 与本地传输**

- [ ] Git Hooks、Rust Helper、IPC、Spool。
- [ ] Amend / Rebase 处理、Worktree、Hook 冲突处理、卸载恢复。
- [ ] 历史导入。

**Achievement**

- [ ] 永久成就、每日等级、每日里程碑、Hidden Achievements。
- [ ] 用户自定义成就、历史回填。

**Desktop**

- [ ] Dashboard、Achievement Gallery（新解锁标记）、Activity & Statistics。
- [ ] Repository Manager、Settings、About & Diagnostics、系统托盘。
- [ ] 平台兼容性矩阵与降级链。

**Notification**

- [ ] Outbox、通知所有权协议与状态机、ACK 恢复、防重复展示。
- [ ] Sound Manager、Asset Manager、Popup Editor。

**Browser**

- [ ] Browser Only、IndexedDB、GitHub 活动识别。
- [ ] Native Messaging、Connected / Queued / Standalone、Service Worker 恢复。

**Productization**

- [ ] 安装、卸载、数据备份恢复、自动更新。
- [ ] CI/CD、发布文档、外部账户清单。

**Documentation**

- [ ] README、AGENTS.md、STATUS.md、架构文档、统计规则文档。
- [ ] Native Messaging 协议文档、通知状态机文档、Git 行为实验记录。
- [ ] 安装与卸载文档、开发文档、发布文档。

---
