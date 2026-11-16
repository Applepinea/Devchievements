# 第 30 章　Phase 0：项目基础与技术验证

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 30. Phase 0：项目基础与技术验证

目标：验证架构可行性，建立工程基础。本阶段不要求任何用户可见的成就功能。

## 30.1 项目初始化

- 检查当前目录、已有文件、Git 状态、开发环境与工具链；不得覆盖用户已有文件。
- 初始化 Monorepo、pnpm Workspace、Cargo Workspace 与目录结构。
- 创建根 AGENTS.md、STATUS.md；把本规范拆分到 `docs/spec/`；建立 `docs/decisions/`（ADR）与 `docs/development/`。
- 建立基础 CI：格式检查、静态检查、单元测试、构建。

## 30.2 技术验证

- Tauri 2 启动；React + TypeScript；WXT + MV3 构建。
- Native Messaging 最小通信（在真实浏览器中验证）。
- SQLite + SQLx 读写与迁移。
- Rust Core 编译与 WASM spike（含 MV3 CSP 中 `wasm-unsafe-eval` 的实际验证）。
- Helper 实现方案对比（直接读 `.git` 文件 / gix 或 git2 / 调用 git 命令）：以实测 Hook 耗时为依据，写入 ADR。
- 当前环境可测的平台桌面通知能力；其余平台标记为 Platform Unverified。

## 30.3 Git 行为实验

按「Amend 与 post-rewrite 时序」与「Git 版本策略」执行全部实验：普通 Commit、Amend、Rebase、交互式 Rebase、Amend 后立即连续 Commit、Companion 在线与离线。

- 在 CI 中跨多个 Git 版本运行，确定最低支持的 Git 版本。
- 记录 Reflog 内容、Hook 触发顺序、post-rewrite 映射、各版本差异；验证分类等待窗口默认值 300 ms 是否合适。
- 实验结果同时输出为 `ADR` 和可重放的 trace fixtures。

Git 行为实验的范围还包括：Submodule 与 Sparse Checkout（至少明确是否支持及降级策略）、共享 `core.hooksPath`、已有旧版本 Devchievements Hook 的升级。

## 30.4 外部账户清单

创建 `docs/development/external-accounts-and-secrets.md`（内容要求见「外部账户与密钥清单」）。

## 30.5 验收

工具链正常；项目可构建；Tauri 可运行；扩展可构建；SQLite 可读写；Native Messaging 最小验证通过；WASM 决策明确；Git 行为实验完成并产出 trace fixtures；关键 ADR 已记录。

此外，以下 ADR 必须在 Phase 0 结束时有明确结论：WASM 方案、Hook 实现方案、最低 Git 版本、分类等待窗口、时区策略；其余 ADR 可以是 Proposed 状态。

---
