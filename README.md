# Devchievements

**Achievements for Developers**

Devchievements 是一个跨平台桌面软件 + 浏览器扩展，把开发者的 Git 活动转化为游戏化的成就、每日等级、提示音和可自定义的视觉通知。

> **独立第三方应用。** 与 GitHub 无隶属、认可或赞助关系。
> **本地优先。** 所有活动数据默认只存在本地，遥测默认关闭。

---

## 当前状态：Phase 0（技术验证）

**这个仓库现在还跑不出成就弹窗。** Phase 0 的目标是验证架构可行性、建立工程基础，并产出 ADR 与 Git 行为实验数据。产品功能从 Phase 2a 开始出现。

真实进度请看 **[STATUS.md](STATUS.md)** —— 它是唯一权威进度表。

---

## 仓库结构

```text
devchievements/
├── apps/
│   ├── desktop/          # Tauri 2 + React + TS 桌面 Companion
│   ├── extension/        # WXT + MV3 浏览器扩展
│   └── native-host/      # Native Messaging Host（Rust）
├── crates/               # Rust Core Engine 与各领域模块
├── packages/             # 共享 TS 类型 / UI / 配置 / Golden Tests
├── docs/
│   ├── spec/             # 产品规范拆分件（逐字一致）
│   ├── decisions/        # ADR
│   └── development/      # 环境基线、兼容性矩阵、启动清单
├── scripts/              # 开发环境脚本、Git 行为实验
├── tests/                # 集成测试
└── AGENTS.md  STATUS.md
```

---

## 快速开始（开发机）

```powershell
Set-Location <repo>
. .\scripts\env.ps1                   # 下发 Rust / Node / pnpm / git / cargo 环境
. .\scripts\start-crates-proxy.ps1    # 启动本地 crates 代理（本机 schannel 缺陷的 workaround）

cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

> ⚠️ 本仓库的 Rust 工具链**安装在仓库外的 `../_toolchain%%**，未写入系统环境变量。
> 具体原因与本机限制见 [environment-baseline.md](docs/development/environment-baseline.md)。

---

## 文档入口

| 文档 | 用途 |
| --- | --- |
| [AGENTS.md](AGENTS.md) | 所有 Agent 的强制约束 |
| [STATUS.md](STATUS.md) | 唯一权威进度表 |
| [docs/spec/README.md](docs/spec/README.md) | 产品规范章节索引 |
| [environment-baseline.md](docs/development/environment-baseline.md) | 本机环境事实 |
| [startup-checklist.md](docs/development/startup-checklist.md) | 每次开工的自检清单 |
| [platform-matrix.md](docs/development/platform-matrix.md) | 平台兼容性矩阵 |
| [docs/decisions/README.md](docs/decisions/README.md) | ADR 索引 |

---

## 技术栈

Tauri 2 · Rust · React · TypeScript · WXT · Manifest V3 · SQLite + SQLx · IndexedDB · Native Messaging · WASM

---

## 许可证

尚未确定（`UNLICENSED%%）。公开发布前需用户确认。
