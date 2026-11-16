# AGENTS.md — Devchievements

> 本文件是**所有 Coding Agent 在本仓库工作的唯一强制入口**。
> 与 `docs/spec/` 下的产品规范冲突时，以规范为准；本文件负责把规范落成可执行约束。
> 任何 Agent 在提交代码前必须读完本文件。

---

## 1. 产品定位

**Devchievements**（内部代号 GSA，仅限开发/目录/内部讨论，**不得出现在任何面向用户的品牌材料中**）是一个**本地优先（local-first）的开发者成就伴侣**：

- 追踪本地 Git 活动与受支持的 GitHub 工作流活动；
- 转化为永久成就、每日等级、每日里程碑、连续活跃记录；
- 以游戏风格通知（弹窗 + 提示音 + 图标）庆祝；
- 所有活动数据默认**只存在本地**；
- 是**独立第三方软件**，与 GitHub 官方 **没有任何** 隶属、认可或赞助关系；
- **不得**使用 GitHub / Steam 的官方 Logo、商标资产，**不得**在商店文案中出现 "Steam"。

Tagline：`Achievements for Developers`。

---

## 2. 项目结构

```text
devchievements/
├── apps/
│   ├── desktop/          # Tauri 2 + React + TS 桌面 Companion
│   ├── extension/        # WXT + MV3 浏览器扩展
│   └── native-host/      # Native Messaging Host（Rust）
├── crates/
│   ├── core/             # Engine：纯逻辑层（无 IO / 无时钟 / 无随机）
│   ├── events/           # 标准事件模型、验证、去重键
│   ├── achievements/     # 永久成就规则与版本化
│   ├── daily-rank/       # 每日等级与每日里程碑
│   ├── notifications/    # Outbox 状态机、通知所有权协议（纯逻辑）
│   ├── sound/            # 音效选择与优先级
│   ├── assets/           # 资源校验与回退
│   ├── filters/          # 用户过滤器与统计口径
│   ├── themes/           # 主题包校验
│   ├── platform/         # 平台能力探测与降级链
│   ├── storage/          # SQLite + SQLx Adapter / IndexedDB Adapter 接口
│   └── git-hooks/        # Helper 与 Hook 安装/链式/卸载
├── packages/
│   ├── shared-types/     # TS 侧与 Rust 对应的类型定义（由 schema 生成/校验）
│   ├── ui/               # 共享 React 组件（桌面 + 扩展）
│   ├── config/           # 共享构建/校验配置
│   └── golden-tests/     # Golden Tests 与 fixtures/git-traces/
├── docs/
│   ├── spec/             # 产品规范拆分件（只读参考，改动需 ADR）
│   ├── decisions/        # ADR
│   └── development/      # 开发文档、兼容性矩阵、外部账户清单
├── tests/                # 集成测试与真机测试脚本
├── scripts/              # 实验、性能测量、构建辅助
├── AGENTS.md  STATUS.md  README.md
└── Cargo.toml  pnpm-workspace.yaml
```

---

## 3. 技术栈（冻结）

| 模块 | 技术 |
| --- | --- |
| Desktop | Tauri 2、Rust、React、TypeScript、CSS、Framer Motion |
| Browser | WXT、Manifest V3、TypeScript、React |
| Storage | SQLite + SQLx（Desktop）；IndexedDB（Browser Only） |
| Communication | Native Messaging（扩展 ↔ Companion）；本地 IPC（Helper ↔ Companion） |
| Workspace | pnpm Workspace + Cargo Workspace |
| Core | Rust Core Engine，尽可能 WASM 共享到浏览器 |
| Testing | Rust Tests、Vitest、Playwright、Golden Tests |
| CI/CD | GitHub Actions |

**硬约束**：
- 同一项目内**禁止**混用 SQLx 与 rusqlite，除非有 ADR 证明必要。
- 禁止使用无保护的 localhost HTTP 替代 Native Messaging 或 IPC。
- 不依赖 Node.js / Python 实现 Git Hook（必须用 Rust Helper）。

---

## 4. 开发与测试命令

### 4.1 环境准备（本机专属，务必先读）

本仓库的开发工具链**安装在工作区内**（`_toolchain/`），未写入用户环境变量。每次开新 Shell 必须：

```powershell
. .\scripts\env.ps1
```

它设置：`RUSTUP_HOME`、`CARGO_HOME`、`PATH`、`PNPM`（DSH 内置 pnpm 的 node 调用方式）、
`NPM_CONFIG_CACHE`（本机 `%LOCALAPPDATA%\npm-cache` 不可写，必须重定向）。

详细环境证据见 [docs/development/environment-baseline.md](docs/development/environment-baseline.md)。

### 4.2 常用命令

**最重要的一条**：验证整个仓库用

```powershell
node scripts/verify.mjs
```

它会依次跑 fmt / clippy / test / build / build-wasm / extension-build，并把日志写入 `.verify/`。
STATUS.md 引用的就是它的输出。

底层命令（环境由 `scripts/env.ps1` 配好）：

```powershell
# Rust（推荐用脚本，避免 PowerShell 吞掉 cargo 输出）
node scripts/cargo-run.mjs fmt --all -- --check
node scripts/cargo-run.mjs clippy --workspace --all-targets -- -D warnings
node scripts/cargo-run.mjs test --workspace
node scripts/cargo-run.mjs build --workspace
node scripts/cargo-run.mjs build -p devchievements-core --target wasm32-unknown-unknown --release

# 前端 / 扩展
pnpm install --ignore-scripts        # 沙箱禁止生命周期脚本
pnpm --filter @devchievements/extension build

# Git 行为实验（生成 .out/ 与 fixtures 原始数据）
node scripts/git-experiments/run-experiments.mjs
node scripts/git-experiments/bench-three-approaches.mjs

# WASM spike
node scripts/wasm-spike/run-wasm-spike.mjs
```

### 4.2.1 本机必须知道的四件事

1. **PowerShell 会吞掉 cargo 的 stderr** → 一律用 `scripts/cargo-run.mjs` 或 `scripts/verify.mjs`。
2. **沙箱禁止管道式 stdio** → 任何要捕获子进程输出的脚本必须用**文件重定向**。
   曾经因为 `spawnSync({encoding:'utf8'})` 静默返回空串，导致依赖解析「假装成功」。
3. **`.ps1` 必须纯 ASCII**（PowerShell 5.1 + GBK）。
4. **pnpm 需 `--ignore-scripts` + `node-linker=hoisted`**，且要手动跑 `wxt prepare`。

### 4.3 提交流程（每个 Phase 内的每个变更都要走完）

1. 检查当前状态 → 2. 明确目标 → 3. 先写/更新设计文档 → 4. 实现 →
5. `cargo fmt` / `pnpm lint` → 6. `cargo test` / `pnpm test` → 7. 构建 →
8. 修问题 → 9. 更新文档与 `STATUS.md` → 10. 记录未解决问题 → 11. 汇报。

---

## 5. 代码风格

- Rust：`rustfmt` 默认配置 + `clippy -D warnings`。公共 API 必须有文档注释。
- TypeScript：`strict: true`，禁止 `any`（必要时 `unknown` + 类型守卫）。禁止非空断言 `!` 除非有注释说明。
- 命名：Rust 用 `snake_case` / `PascalCase`；TS 用 `camelCase` / `PascalCase`；数据库列用 `snake_case`。
- 错误：Rust 用 `thiserror` 定义领域错误，禁止 `unwrap()` 出现在非测试代码路径（`expect()` 必须写明不变量）。
- 时间：**任何业务代码不得直接调用 `SystemTime::now()` / `Date.now()`**，必须通过注入的 `ClockProvider`。
- ID：**不得直接生成随机 ID**，必须通过注入的 `IdGenerator`。
- 注释写「为什么」，不写「做了什么」。ADR 级决策写进 `docs/decisions/`，不要塞进代码注释。

---

## 6. 模块边界（不可越界）

| 模块 | 可以做 | **绝对不可以做** |
| --- | --- | --- |
| `crates/core`（Engine） | 纯计算、状态变更、副作用意图（`side_effect_intents`） | 访问系统时钟、随机数、数据库、OS 通知、网络、文件系统 |
| `crates/storage` | SQLite/IndexedDB 事务、投影读写、迁移 | 包含成就判定逻辑（判定在 Engine） |
| `crates/notifications` | Outbox 状态机、Offer/Claim/ACK 转换 | 真正弹窗、播声音（那是 Consumer 的事） |
| `crates/git-hooks` | Helper 采集、Hook 安装/链式/卸载、IPC/Spool 投递 | 判定成就、写数据库、发通知 |
| `apps/desktop` | 窗口、UI、托盘、调用 Engine 与 Consumer | 直接写 SQL 绕过 storage 层 |
| `apps/extension` | 页面识别、IndexedDB、浏览器通知 | Full Experience 下自行修改权威统计（Companion 唯一权威） |

**Engine 契约（不得违反）**：

```text
Engine(state_snapshot, event, injected_context) -> { state_changes, side_effect_intents }
```

- Engine 输出与 Outbox 意图必须由 Storage Adapter 在**同一事务**内原子应用。
- 事务提交点必须位于 Engine 输出**之后**、任何外部副作用**之前**。

---

## 7. 安全约束（硬性）

1. **Token / 密钥**只能用操作系统安全存储；**禁止**写入普通配置文件、Git 仓库、日志、错误信息、调试输出。
2. IPC（Named Pipe / Unix Domain Socket）**只能**做事件投递：不执行命令、不携带 Token、不做远程访问。
3. Unix Socket 目录 `0700`、Socket `0600`；Windows Named Pipe ACL 限当前用户；平台支持时校验对端凭据。
4. MAIN world 注入必须走受控消息桥，校验来源、结构、长度、类型与超时。
5. 用户导入的图片/音效/主题：**禁止执行其中任何脚本或代码**；校验真实文件类型（读魔数，不信任扩展名）；限制大小。
6. 自定义成就 JSON：必须有 Schema、类型校验、**最大嵌套深度 5**、**最大条件数 50**、**确定性步数预算**（不得使用墙钟超时）、禁止互相引用、禁止任意代码执行。
7. 诊断包导出默认脱敏；日志默认不含 Commit Message 与 Token。
8. **禁止**自行创建付费账户、购买证书/域名、使用用户私人密钥、代替用户接受法律协议。

---

## 8. 隐私要求

- 默认**全部本地**。**不得**默认上传 Git 历史、Commit Message、仓库名、PR 标题、Issue 内容、活动记录。
- 遥测**默认关闭**，开启必须显式同意，且必须可随时关闭。
- Privacy Mode：隐藏仓库名、Commit Message、PR 标题，使用通用文案，隐藏锁屏预览敏感信息。
- **不得**宣称 Privacy Mode 等于数据加密。
- Privacy Mode 同样作用于诊断页与日志。

---

## 9. Phase 顺序与人工检查点

```text
Phase 0 → Phase 1 → Phase 2a → 【检查点 A】 → Phase 2b → 【检查点 B】
→ Phase 3a → Phase 3b → 【检查点 C】 → Phase 4a → Phase 4b
```

| 检查点 | 时机 | 必须展示 |
| --- | --- | --- |
| **A（v0.1-alpha）** | Phase 2a 完成 | 本地 Git → 弹窗 + 提示音的真实闭环、Spool 恢复、Hook 安装/卸载、最小诊断视图、测试报告、已知问题、STATUS.md |
| **B（v0.1）** | Phase 2b 完成 | 完整 Desktop Only 产品、全部页面、历史导入、安装/卸载、兼容性矩阵现状 |
| **C** | Phase 3b 完成 | Desktop Only / Browser Only / Full Experience 三态、断线重连、通知所有权握手、完整测试报告 |

**铁律**：
- 没有用户**明确**确认，**不得跨越任何检查点**。
- 用户确认检查点 B 之前，**不得开始浏览器扩展开发**。
- Phase 3a 与 3b 必须**分别**验收；Browser Only 完成 ≠ Full Experience 完成。
- 等待确认期间可以继续做不依赖该决策的文档、测试、修复。

---

## 10. 验收要求（防止虚假完成）

`STATUS.md` 的状态取值严格限定：
`Not Started` / `In Progress` / `Implemented` / `Tested` / `Partially Implemented` / `Platform Unverified` / `Blocked`。

**必须区分**这五件事，不得混淆：

1. 代码已编写 → `Implemented`
2. 构建已通过 → 仍然只是 `Implemented`（加备注）
3. 自动化测试已通过 → `Tested`
4. 实际运行测试已通过 → `Tested`（加备注「真机/真流程」）
5. 平台兼容性已验证 → 才能更新兼容性矩阵

铁律：
- **不得**把尚未实现的功能标记为已完成。
- **不得**把仅通过编译的功能标记为已通过实际运行测试。
- **不得**用静态假数据冒充真实数据（所有页面必须从真实数据库读取）。
- **不得**编造性能数据。没有实测的指标一律标记为未验证。
- 当前环境无法验证的平台/能力 → `Platform Unverified`，并在 CI 中至少完成构建验证。

---

## 11. 用户确认边界

以下操作**必须先获得用户明确确认**：
重大架构变化；不可逆数据库迁移；覆盖现有文件；删除用户数据；购买服务；创建付费账户；使用用户私人密钥；上传用户数据；改变隐私或遥测策略；跳过人工检查点；改变核心统计规则。

普通实现细节由 Agent 自主决定，但重要决策必须写入 ADR。

---

## 12. 子目录约定

子目录可以有各自的 `AGENTS.md`（例如 `apps/extension/AGENTS.md` 记录 WXT 特有约定），但**不得与本文件冲突**。冲突时以本文件为准。
