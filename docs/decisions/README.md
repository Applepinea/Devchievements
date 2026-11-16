# 架构决策记录（ADR）

本目录记录 Devchievements 的**重大技术决策**。

## 规则

1. **什么必须写 ADR**：影响两个及以上模块、难以回退、或规范明确要求出结论的决策。
   规范 §43.3 点名要求至少覆盖：WASM 方案；数据模型；Git Hook 策略与 Helper 实现方案；
   Amend / Rebase 处理与分类等待窗口；最低支持的 Git 版本；IPC 与 Spool 设计；
   Outbox 与通知所有权状态机；Native Messaging 协议；时区策略。
2. **状态取值**：`Proposed` / `Accepted` / `Rejected` / `Superseded by ADR-XXXX`。
3. **不可篡改**：已 Accepted 的 ADR **不改内容**，只能被新的 ADR 取代（保留历史）。
4. **必须带证据**：没有实测证据的结论要显式写「未验证 / Assume」，不得写得像已验证。
5. 新增 ADR 请复制 [0000-template.md](0000-template.md)，编号递增。

## Phase 0 结束时必须有明确结论的 ADR（规范 §30.5）

| ADR | 主题 | 状态 |
| --- | --- | --- |
| [0001](0001-wasm-strategy.md) | WASM 方案 | Proposed |
| [0002](0002-hook-helper-implementation.md) | Hook 实现方案（直读 .git / gix / git2 / 调 git 命令） | Proposed |
| [0003](0003-minimum-git-version.md) | 最低支持的 Git 版本 | Proposed |
| [0004](0004-rewrite-classification-window.md) | Amend / Rebase 分类等待窗口 | Proposed |
| [0005](0005-timezone-strategy.md) | 时区策略 | Proposed |

## 其余规划的 ADR

| ADR | 主题 | 目标 Phase |
| --- | --- | --- |
| 0006 | 数据模型与投影设计 | Phase 1 |
| 0007 | IPC 与 Spool 设计 | Phase 1/2a |
| 0008 | Outbox 与通知所有权状态机 | Phase 1 |
| 0009 | Native Messaging 协议与版本协商 | Phase 3b |
| 0010 | 事件去重与 Rewrite 关联 | Phase 1 |
| 0011 | 浏览器侧 Engine（WASM 一致性） | Phase 3a |
| 0012 | 本地工具链隔离安装（本仓库专属） | Phase 0 ✅ 已 Accepted |
