# Devchievements

**Achievements for Developers**

Devchievements 是一个跨平台桌面软件 + 浏览器扩展，把开发者的 Git 活动转化为游戏化的成就、每日等级、提示音和可自定义的视觉通知。

> **独立第三方应用。** 与 GitHub 无隶属、认可或赞助关系。
> **本地优先。** 所有活动数据默认只存在本地，遥测默认关闭。

---

## 当前状态：Phase 0（技术验证）

**这个仓库现在还跑不出成就弹窗。** Phase 0 的目标是验证架构可行性、建立工程基础，并产出 ADR 与 Git 行为实验数据。产品功能从 Phase 2a 开始出现。

真实进度见 **[STATUS.md](STATUS.md)** —— 它是唯一权威进度表。

---

## 快速开始

```powershell
# 1. 一次性：重建隔离工具链（幂等；约 1.6 GB，全部在 ../_toolchain 内）
. .\scripts\install-toolchain.ps1

# 2. 每个新 Shell：下发环境变量
. .\scripts\env.ps1

# 3. 每个新 Shell：启动本地 crates 代理（本机 schannel 缺陷的绕过）
. .\scripts\start-crates-proxy.ps1

# 4. 验证一切
node scripts/verify.mjs
```

预期输出：

```text
PASS  fmt / clippy / test / build / build-wasm / extension-build
ALL CHECKS PASSED
```

> ⚠️ Rust 工具链装在**仓库外**的 `../_toolchain```，未写入系统环境变量。
> 原因（无管理员权限、无 MSVC、schannel 损坏）见 [ADR-0012](docs/decisions/0012-isolated-dev-toolchain.md)。

---

## 仓库结构

```text
devchievements/
├── apps/
│   ├── desktop/          # Tauri 2 + React + TS（Phase 2a 起）
│   ├── extension/        # WXT + MV3 浏览器扩展
│   └── native-host/      # Native Messaging Host（Rust）
├── crates/
│   ├── core/             # Engine：纯逻辑，可编译到 native 与 wasm32
│   ├── git-hooks/        # 直读 .git 的 Helper（含 inflate 与 commit 解析）
│   ├── storage/          # SQLite + SQLx 适配器
│   └── ...               # events / achievements / daily-rank / notifications / ...
├── packages/
│   └── golden-tests/fixtures/git-traces/   # 15 份可回放的 Git 行为 trace
├── docs/
│   ├── spec/             # 产品规范拆分件（逐字一致）
│   ├── decisions/        # ADR
│   └── development/      # 环境基线、风险分析、实验记录、兼容性矩阵
├── scripts/              # 环境脚本、Git 实验、WASM spike、verify.mjs
└── AGENTS.md  STATUS.md
```

---

## 文档入口

| 文档 | 用途 |
| --- | --- |
| [AGENTS.md](AGENTS.md) | 所有 Agent 的强制约束 |
| [STATUS.md](STATUS.md) | 唯一权威进度表 |
| [docs/spec/README.md](docs/spec/README.md) | 产品规范章节索引 |
| [environment-baseline.md](docs/development/environment-baseline.md) | 本机环境事实 |
| [risk-analysis.md](docs/development/risk-analysis.md) | 技术风险与当前状态 |
| [git-behavior-experiments.md](docs/development/git-behavior-experiments.md) | 15 场景 Git 行为实验记录 |
| [phase-0-report.md](docs/development/phase-0-report.md) | Phase 0 结果与下一步 |
| [platform-matrix.md](docs/development/platform-matrix.md) | 平台兼容性矩阵 |
| [docs/decisions/README.md](docs/decisions/README.md) | ADR 索引 |
| [startup-checklist.md](docs/development/startup-checklist.md) | 每次开工的自检清单 |

---

## 技术栈

Tauri 2 · Rust · React · TypeScript · WXT · Manifest V3 · SQLite + SQLx · IndexedDB · Native Messaging · WASM

---

## 许可证

尚未确定（`UNLICENSED`）。公开发布前需用户确认。
