# STATUS — Devchievements

> **唯一权威进度表。**
>
> 状态取值严格限定为：
> `Not Started` / `In Progress` / `Implemented` / `Tested` / `Partially Implemented` / `Platform Unverified` / `Blocked`
>
> **不可混淆的五件事**（AGENTS.md §10）：代码已写 / 构建已过 / 自动化测试已过 / 实际运行测试已过 / 平台兼容性已验证。
>
> 最后更新：2026-11-18 —— **Phase 4b 进行中（安装包打通、更新机制接入、发布流水线就位）**

---

## 唯一验证命令

```powershell
node scripts/verify.mjs
```

当前结果（真实执行，非推测）：

```text
PASS  fmt                      1245 ms
PASS  clippy                   3876 ms
PASS  test                    15657 ms   440 tests, 48 suites, 0 failed
PASS  build                    1664 ms
PASS  build-wasm                548 ms
PASS  desktop-ui-build         2476 ms
PASS  desktop-build           11184 ms
PASS  extension-build         10234 ms
PASS  extension-test            218 ms
ALL CHECKS PASSED
```

桌面应用是**独立**的 cargo 工程（不能用 workspace 的 GNU 工具链），所以它的测试不在
`verify.mjs` 里，要单独跑：`scripts/dev/build-desktop.bat test`（当前 6 个测试，
覆盖更新前的数据库快照/回滚与公钥漂移检查）。

原始日志与机器可读摘要：`.verify/`

---

## 当前阶段

**Phase 0：项目基础与技术验证** —— `Partially Implemented`（已完成，见下）
**Phase 1：Core Engine 与数据层** —— `Tested`（完成）
**Phase 2a：本地 Git 到通知的真实闭环** —— `Partially Implemented`（Toast/UI 未验证）
**Phase 2b：完整 Desktop Only 产品** —— `Partially Implemented`（跨平台仅 Windows 实测）

**下一个门禁：检查点 A**（需 Phase 2a 完成）。当前**没有任何检查点被跨越。**

---

## Phase 3b 任务表（Full Experience：扩展检测 + 桌面判定）

> 2026-11-17 产品决定：扩展只负责**检测**，判定与弹窗由本地桌面应用完成。
> 这条改变了 §36（Browser Only）的边界，见 ADR-0018。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 3b.1 | Native Messaging 协议（4 字节长度 + JSON） | `Tested` | `native-host` e2e 11 项：真实管道、真实分帧 |
| 3b.2 | 浏览器报告 → 本地判定 → 入库 | `Tested` | 同一 e2e：companion drain 后诊断显示 1 个提交 |
| 3b.3 | 扩展侧转发 + 本地留痕 | `Tested` | `extension-load` 18 项：真实 `connectNative` 握手 + 本地存储 |
| 3b.4 | Host 清单与浏览器注册（固定扩展 ID） | `Tested` | `register-native-host.mjs` 写清单 + HKCU 注册；ID 由提交的公钥推导并与加载一致 |
| 3b.5 | 桌面弹窗呈现浏览器事件 | `Tested` | `browser-to-popup` e2e 6 项：浏览器字节 → host → 桌面弹窗 |
| 3b.6 | 注册与清理脚本 | `Tested` | `register-native-host.mjs` / `unregister-native-host.mjs`，只动 HKCU |
| 3b.7 | PR / Issue 识别 | `Tested` | 扩展 14 项测试；Rust 分类 opened/merged/closed；host 转发标志位 |

---

## Phase 3a 任务表（Browser Only）

> 检查点 B 已由用户以「继续 phase3」明确批准。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 3a.1 | 扩展可独立安装并在真实浏览器运行 | `Tested` | `extension-load` e2e 7 项；CDP `Extensions.loadUnpacked` |
| 3a.2 | Rust 核心在 MV3 worker 内实例化（CSP） | `Tested` | 同一脚本断言 WASM 实例化与探针调用 |
| 3a.3 | GitHub 活动识别（Commit/Release） | `Tested` | 9 个单元测试（URL/日期/去重键）+ `extension-load` 端到端入库 |
| 3a.4 | IndexedDB 持久化 + 独立 Outbox | `Partially Implemented` | IndexedDB 存储与去重已端到端验证；**Outbox 队列尚未实现** |
| 3a.5 | 浏览器通知与音效 | `Not Started` | |
| 3a.6 | 浏览器端设置/Dashboard/图鉴 | `Partially Implemented` | Dashboard 读 IndexedDB 并渲染（端到端 3 项）；**设置与图鉴未做** |
| 3a.7 | 仓库身份映射（remote → canonical） | `Tested` | 钩子记录 remote；hook 与 browser 归并为同一仓库；无 remote 保持 local |
| 3a.9 | 用户身份映射（GitHub Login → Email） | `Tested` | 设置界面可编辑；`login-mapping` e2e 7 项（未映射拒绝 → 映射后计数） |
| 3a.8 | Service Worker 回收后状态恢复 | `Tested` | `service-worker-restart` e2e：真实停掉 worker 后数据仍在 |

---

## Phase 2b 任务表

> 用户已明确指示进入 Phase 2b。跨检查点时的未验证项见
> [phase-2b-progress.md](docs/development/phase-2b-progress.md) 第 0 节。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 2b.1 | 平台能力探测与降级链（§29） | `Tested` | 13 个测试；运行时实测降级到 in-app-only |
| 2b.2 | 兼容性矩阵（§29.6） | `Tested` | Windows 行实测；macOS/Linux 标为「不支持（ADR-0017）」 |
| 2b.3 | Dashboard / Gallery / Activity / Repository Manager / About | `Tested` | 5 个页面全部读真实数据库；`pages` e2e 18 项通过 |
| 2b.4 | 历史导入界面与完整流程 | `Tested` | 管线 + Repository Manager + **向导导入步骤**；静默/幂等/跨路径去重/原始日期已测 |
| 2b.5 | 基础 Settings | `Tested` | 身份/通知开关/音量与静音/隐私/运行模式/时区/数据管理（打开目录、备份、清除）；`notification-settings` e2e 12 项 |
| 2b.6 | 安装向导 / 卸载机制 | `Tested` | 三步向导（身份→钩子→完成）+ 残留 Hook 静默退出；`wizard` e2e 9 项、`uninstall` 4 项 |
| 2b.7 | 数据备份与恢复 | `Tested` | `backup-restore` e2e 13 项：真备份、真删除、真恢复、数据一致、拒收非数据库 |
| 2b.8 | Outbox 可观测性（延迟分布与原因码） | `Tested` | 7 个测试（含真实数据库）；已并入 `--diagnostics` |
| 2b.9 | 空状态与引导（§34.1 补充） | `Tested` | 每页有空状态；无身份时给出引导；卸载后目录不被复活 |
| 2b.10 | 通知弹窗窗口（§28） | `Tested` | 不抢焦点/不进任务栏/置顶/自动关闭；数据随片段传入 |
| 2b.11 | 升级路径（§34.2） | `Tested` | `upgrade` e2e 13 项：旧配置文件、钩子重复安装、数据库重开 |

---
## Phase 2a 任务表

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 2a.1 | Rust Helper（post-commit / post-rewrite / post-merge） | `Tested` | 真实 `git commit` 触发，端到端通过 |
| 2a.2 | Hook 安装 / 备份 / 链式 / 卸载 | `Tested` | 12 个测试；**shell 链式未真实验证**（见 §32 文档） |
| 2a.3 | IPC 帧协议 | `Tested` | 9 个测试，含超限长度与截断 |
| 2a.4 | Spool 回退 | `Tested` | 11 个测试，含并发与损坏恢复 |
| 2a.5 | 实时 IPC 通道（客户端 + 服务端） | `Platform Unverified` | 已实现并编译；**本机沙箱拒绝命名管道**（os error 5）。回退路径已验证 |
| 2a.6 | Tauri Companion 与托盘 | `Tested` | 编译/链接/托盘运行/真实提交驱动/窗口创建均已验证；托盘含隐私开关、打开窗口、手动 drain |
| 2a.7 | SQLite 持久化接入 Helper | `Tested` | 完整闭环：commit→hook→helper→spool→companion→SQLite |
| 2a.8 | Outbox Consumer（Desktop 子集） | `Tested` | 投递、重试、`delivery_unknown` 均有测试 |
| 2a.9 | 桌面弹窗与提示音 | `Partially Implemented` | **提示音已完成并实测**；Toast 注册与调用已实现（安装器 exit 0），**端到端未通过** |
| 2a.15 | 通知呈现 | `Tested` | **改用自有弹窗**（ADR-0015）：真实提交触发真实窗口，`popup` e2e 8 项通过；系统通知通道已移除 |
| 2a.16 | AppNotificationManager 替代路线 | `Not Started` | 见 [ADR-0016](docs/decisions/0016-notification-api-choice.md) |
| 2a.17 | Sparse Packaging（若确需 Package Identity） | `Not Started` | 见 ADR-0016 |
| 2a.10 | Companion 启动消费 Spool | `Tested` | 无头 Companion 真实消费并计数 |
| 2a.11 | 最小诊断视图 | `Partially Implemented` | **数据侧完成**：`--diagnostics` 输出真实 JSON，仓库别名化、路径脱敏，12 项 e2e 校验；UI 需 WebView2 |
| 2a.14 | 通知队列与合并（§16.4、§16.6） | `Tested` | 一个事件一个弹窗；普通提交只出声不弹窗 |
| 2a.18 | 提示音：内置默认 + 基础播放（§19） | `Tested` | 15 个选择逻辑测试；真实提交实测；奖励音为用户提供的 achievement.wav |
| 2a.13 | Privacy Mode 基础行为（§26.2） | `Tested` | 26 个测试；**托盘开关 + 持久化 + 通知文案实测**（8 项 e2e） |
| 2a.19 | 设置文件（§26.4 版本化配置） | `Tested` | `settings.json`：身份邮箱、隐私模式；未知字段忽略 |
| 2a.12 | Crash Recovery 矩阵（§32.5） | `Partially Implemented` | 6 个场景中 4 个已真实通过；2 个需实时 IPC |

> 详见 [phase-2a-progress.md](docs/development/phase-2a-progress.md)。
> 端到端：`node scripts/e2e/hook-to-spool.mjs`、`node scripts/e2e/full-loop.mjs`、
> `node scripts/e2e/ipc-fallback.mjs`、`node scripts/e2e/crash-recovery.mjs`

---

## Phase 1 任务表（已完成）

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1.1 | 标准事件模型与验证 | `Tested` | `crates/events`，31 个测试 |
| 1.2 | 来源可信度 | `Tested` | 含「unverified 不计入官方统计」断言 |
| 1.3 | 仓库身份识别 | `Tested` | SSH/HTTPS/git 后缀/大小写/跨平台不合并 |
| 1.4 | Commit 去重键 | `Tested` | `repo_id + sha`；patch id 仅辅助 |
| 1.5 | 用户身份识别（多邮箱） | `Tested` | 身份模型 + 优先级 + GitHub login 映射 + Git 配置读取；**未配置时禁止统计已强制** |
| 1.6 | 时区系统 + 日界线 | `Tested` | 内置 TZDB；DST 与 04:00 边界已测 |
| 1.7 | 每日等级与每日里程碑 | `Tested` | 14 个测试；用户阈值不影响官方口径 |
| 1.8 | 永久成就（含 required_capabilities） | `Tested` | 13 个测试；能力由条件推导 |
| 1.9 | 用户过滤器 | `Tested` | 18 个测试；官方口径与用户口径在类型上分离 |
| 1.10 | 统计投影 | `Tested` | 读写 + 可重建性均已测（`rebuild_from_events` 复现同值） |
| 1.11 | Engine（含读集） | `Tested` | 7 个测试；读集有断言守护 |
| 1.12 | SQLite Storage Adapter 与 SQLx 迁移 | `Tested` | 版本化 schema + 12 个真实库测试 |
| 1.13 | 事件持久化 | `Tested` | 事件+投影+解锁+Outbox **同一事务**，有测试守护 |
| 1.14 | Outbox 与通知所有权状态机（纯逻辑） | `Tested` | 18 个测试；合法转换表 + 防重复展示 |
| 1.15 | 历史导入管线（引擎侧） | `Tested` | 用提交时间归档、幂等、可续传；跨日用例已测 |
| 1.16 | Golden Tests | `Tested` | 16 个测试回放 15 份真机 trace |
| 1.17 | 属性测试 | `Tested` | 11 个属性（时区 5 / Hook 顺序 6），各上千用例 |
| 1.18 | packfile 解析 | `Tested` | **本轮解除风险 A3**：真实 gc 仓库上 10 个历史提交可读。见 [ADR-0013](docs/decisions/0013-packfile-support.md) |
| 1.19 | IndexedDB Adapter 接口设计 | `Implemented` | 只有 trait 与对象存储名常量（§31.1 要求「不展开」） |
| 1.20 | 初步性能基准 | `Tested` | 摄取 p95 **3.12 ms**，n=2000，release |
| 1.21 | Achievement Builder 复杂度与确定性限制 | `Tested` | 深度 5 / 条件 50 / **步数预算**（非墙钟）；13 个测试 |

> 详见 [phase-1-progress.md](docs/development/phase-1-progress.md)。

---

## 当前阶段（Phase 0 归档）

**Phase 0：项目基础与技术验证** —— 状态 `Partially Implemented`

**下一个门禁：检查点 A**（需 Phase 2a 完成）。当前**没有任何检查点被跨越。**

---

## Phase 0 任务表

| # | 任务 | 状态 | 证据 / 备注 |
| --- | --- | --- | --- |
| 0.1 | 检查目录、Git 状态、工具链 | `Tested` | [environment-baseline.md](docs/development/environment-baseline.md) |
| 0.2 | Monorepo 初始化 | `Tested` | `cargo build --workspace` 通过 |
| 0.3 | 根 AGENTS.md | `Implemented` | [AGENTS.md](AGENTS.md) |
| 0.4 | 根 STATUS.md | `Implemented` | 本文件 |
| 0.5 | 规范拆分 docs/spec/ | `Tested` | 49 章 → 47 文件，覆盖率断言通过 |
| 0.6 | ADR 框架与 docs/development/ | `Implemented` | 7 份有效 ADR + 模板 + 索引 |
| 0.7 | 基础 CI | `Implemented` | `.github/workflows/ci.yml` 已写，**从未运行过**（无仓库） |
| 0.8 | Tauri 2 启动 + React + TS | `Blocked` | 无 MSVC；GNU 目标不支持 Tauri Windows 打包。见 [ADR-0012](docs/decisions/0012-isolated-dev-toolchain.md) |
| 0.9 | WXT + MV3 构建 | `Tested` | WXT 0.20.27，`verify.mjs` 每次都会重建 |
| 0.10 | Native Messaging 最小通信 | `Implemented` | Rust 侧 framing 已实现并有单测；**浏览器联调未做**（沙箱） |
| 0.11 | **SQLite + SQLx 读写与迁移** | `Tested` | **本轮解除阻塞**：5 个真实数据库测试全通过（往返、幂等、事务回滚、级联、重开保留） |
| 0.12 | Rust Core 编译为 WASM | `Tested` | 27,569 bytes；实例化 0.606 ms；与 native 逐字节一致 |
| 0.13 | MV3 CSP `wasm-unsafe-eval` 真实浏览器验证 | `Platform Unverified` | manifest 含该 CSP 且 wasm 已打包；**浏览器无法启动** |
| 0.14 | Helper 实现方案对比 | `Tested` | **本轮补齐方案 B**：三方案全部实测。见 [ADR-0002](docs/decisions/0002-hook-helper-implementation.md) |
| 0.15 | Git 行为实验（15 场景） | `Tested` | [git-behavior-experiments.md](docs/development/git-behavior-experiments.md) |
| 0.16 | 最低 Git 版本结论 | `Not Started` | 只有 2.54.0 一个版本的数据。见 [ADR-0003](docs/decisions/0003-minimum-git-version.md) |
| 0.17 | 分类等待窗口 | `Tested` | 实测 436–452 ms → 改 800 ms。见 [ADR-0004](docs/decisions/0004-rewrite-classification-window.md) |
| 0.18 | trace fixtures + JSON Schema | `Tested` | 15 份 fixture，Schema 与 Rust 侧双重校验 |
| 0.19 | 桌面上通知能力实测 | `Not Started` | Phase 0 无 Companion，无法验证 |
| 0.20 | `external-accounts-and-secrets.md` | `Implemented` | [链接](docs/development/external-accounts-and-secrets.md) |
| 0.21 | 时区策略结论 | `Implemented` | [ADR-0005](docs/decisions/0005-timezone-strategy.md) |
| 0.22 | **风险分析（§48.3）** | `Implemented` | **本轮补写**：[risk-analysis.md](docs/development/risk-analysis.md) |
| 0.23 | **Phase 0 实施计划（§48.5）** | `Implemented` | **本轮补写**：[phase-0-plan.md](docs/development/phase-0-plan.md) |

---

## Phase 0 强制 ADR 结论（规范 §30.5）

| ADR | 主题 | 状态 | 证据 |
| --- | --- | --- | --- |
| [0001](docs/decisions/0001-wasm-strategy.md) | WASM 方案 | `Partially Accepted` | 4/6 验证通过；浏览器内加载未验证 |
| [0002](docs/decisions/0002-hook-helper-implementation.md) | Hook 实现方案 | `Accepted` | ✅ **三方案全部实测**（A 38.7 / B 66.2 / C 13.4 ms p95） |
| [0003](docs/decisions/0003-minimum-git-version.md) | 最低 Git 版本 | `Proposed` | ❌ 单一版本，证据不足 |
| [0004](docs/decisions/0004-rewrite-classification-window.md) | 分类等待窗口 | `Accepted`（数值 Provisional） | ✅ 三次实测一致 |
| [0005](docs/decisions/0005-timezone-strategy.md) | 时区策略 | `Accepted`（选型） | 选型有据；实现待 Phase 1 |

**诚实说明**：规范要求 5 项全部有明确结论。实际达成 3 项 Accepted、1 项 Partially Accepted、
1 项因**只有一个 Git 版本的数据**保持 Proposed。编造最低版本会直接违反 §44.2。

---

## 实测数字汇总

| 指标 | 实测值 | 目标 | 判定 |
| --- | --- | --- | --- |
| Hook 耗时（直读 .git，p95） | **13.40 ms** | < 50 ms | ✅ |
| Hook 耗时（gix，p95） | 66.16 ms | < 50 ms | ❌ 超预算 |
| Hook 耗时（单次 git，p95） | 38.73 ms | < 50 ms | 🟡 吃掉 77% 预算 |
| Hook 耗时（朴素外派，p95） | 428.26 ms | < 200 ms 硬超时 | ❌ 违反 |
| 自动化测试 | **42 个通过 / 0 失败** | — | ✅ |
| `cargo clippy -D warnings` | 0 警告 | 0 | ✅ |
| WASM 产物 | 27,569 bytes | < 2 MB | ✅ |
| WASM 实例化 | 0.606 ms | — | ✅ |
| native ↔ wasm 输出一致性 | 逐字节相同 | 相同 | ✅ |
| SQLite 真实往返测试 | 5 个通过 | — | ✅ |
| **Hook 耗时（出货 helper，p95，n=200）** | **10.81 ms**（p50 9.35） | < 50 ms | ✅ 2026-11-17 实测 |
| **Commit → 通知 p95（n=200，声音起播）** | **274 / 328 ms**（两次独立运行，p50 ≈ 261） | < 500 ms | ✅ 修掉 500 ms 轮询后 |
| Commit → 弹窗首帧（n=1） | 387–424 ms | < 500 ms | 🟡 样本太少，不作结论 |
| **空闲 CPU（仅托盘，45 s）** | **0.12% 整机 / 2.40% 单核** | < 1% | ✅ 整机口径；**开窗口未测** |
| 修轮询前的 Commit → 通知 p95 | 1004 ms（p50 737） | < 500 ms | ❌ 已修 |

> 每个数字都能在 `.verify/` 或 `scripts/**/.out/*.json` 找到原始样本。未实测的格子明确标注「未测量」。

> 2026-11-18 性能跟进调整了 spool 计数的内存用法、托盘空闲扫描周期、历史导入分配和扩展 IndexedDB 查询。
> 托盘扫描周期改动已**实测并回退**：100 ms 下 commit→通知 p95 升到 437 ms（预算的 87%），
> 50 ms 下为 277 ms，空闲代价 0.195% 整机。取舍与数据见 `docs/development/performance-optimizations.md`。

---

## 平台兼容性

见 [platform-matrix.md](docs/development/platform-matrix.md)。

- 可测：Windows 11（Hook 行为已实测通过）
- **不可测**：macOS、Linux、Brave；浏览器（沙箱禁止启动）

---

## 阻塞项

| 阻塞 | 影响 | 需要什么 |
| --- | --- | --- |
| 无 MSVC、无管理员权限 | **Tauri 无法构建**（其余 Rust 侧已完全可用） | 用户升权安装 VS Build Tools + Windows SDK |
| 沙箱禁止浏览器启动 | MV3 CSP、扩展端到端、Native Messaging 联调 | 沙箱外运行，或手动加载扩展验证 |
| 沙箱禁止 `sh.exe` | shell 钩子链式执行、`git submodule add` | 中立环境复测 |
| 无 GitHub 仓库 | CI 从未运行；多 Git 版本矩阵无法执行 | 用户创建仓库并授权 |

---

## 已知问题（产品层面，未修）

1. ~~**自定义成就永远不会解锁。**~~ **已修复（2026-11-18）。** 原来的原因是：设置页把定义
   写进 `settings.json` 的 `userAchievements`，但没有任何代码把它交给评估路径。
   现在的链路是 `settings → CompanionConfig.user_rules → ingest_commit_with_rules`，
   在同一事务内用**官方统计**判定，并复用内置成就的记录路径（解锁行、outbox、
   导入时静默），所以两者不会出现语义分叉。GUI 在每次 drain 前重读规则，
   删除的规则下一次提交就失效。回归由 `scripts/e2e/user-achievements.mjs` 守住：
   真实 git 提交 → hook → helper → drain → 解锁。
   **仍缺的**：Builder 的表单界面（目前只能写 JSON），所以状态仍是 `Partially Implemented`。
1b. **渲染进程的异常现在会进日志。** 之前 UI 崩了只会得到空白窗口：webview 的控制台
   用户打不开，Rust 侧只知道「没有任何视图报告就绪」。已加 `ui_error` 命令与全局
   `error` / `unhandledrejection` 处理，异常会以 `UI ERROR ...` 出现在 companion.log。
   **这是被一次真实事故逼出来的**：向导里把 `useState` 写在依赖它的 `useEffect` 之后，
   压缩后变成 TDZ（`Cannot access 'y' before initialization`），窗口全白。
2. **成就名与等级名尚未本地化。** 界面已中英双语（4b.6），但内置成就目录
   （`First Commit`、`Common Day`…）和隐私模式通用文案来自 Rust 侧，仍是英文。
   **等成就列表定稿后再做**：现在就翻会在目录变动时白改一轮（用户 2026-11-17 决定）。
   补齐方式是让 Rust 也读语言设置、在生成文案时选语言。
3. **一个坏字段会重置整份设置。** `settings.json` 解析失败时整体回退到默认值，
   所以手改文件时把 `popup` 写成 `{"displaySeconds":1}`（缺必填字段）会连 `emails` 一起丢，
   应用随后退化成「从 git config 猜身份」。同一现象在做性能夹具时踩到。

---

## 已知问题（环境层面，非产品缺陷）

1. **schannel TLS 损坏** → git 用 `http.sslBackend=openssl`；cargo 用本地 sparse 代理。
2. **`.ps1` 必须纯 ASCII**（PowerShell 5.1 + GBK）。
3. **PowerShell 会吞掉 cargo 的 stderr** → 统一用 `scripts/cargo-run.mjs` 或 `scripts/verify.mjs`。
4. **管道式 stdio 被禁** → 所有脚本用**文件重定向**。
   ⚠️ 这个坑实际咬过一次：`spawnSync({encoding:'utf8'})` 静默返回空串，让依赖解析「假装成功」。
5. **pnpm 生命周期脚本被禁** → `--ignore-scripts` + 手动 `wxt prepare`；且需 `node-linker=hoisted```。

---

## 下一步

**需要用户决定（见 [Phase 0 报告](docs/development/phase-0-report.md#5-需要你决定的三件事)）：**

1. 是否提供 MSVC 环境（解锁 Tauri）。
2. 是否在沙箱外补做 MV3 浏览器验证（成本极低）。
3. 是否创建 GitHub 仓库（解锁 CI 与最低 Git 版本结论）。

**Phase 1 可以立刻开工的部分**：事件模型、身份识别、时区、投影与 Engine、每日等级、
永久成就、Outbox 状态机、packfile 支持、Golden Tests。
唯一与 MSVC 无关但**风险最高**的待办是 **packfile 解析**（见 risk-analysis A3）。

---

## 变更日志

| 日期 | 变更 |
| --- | --- |
| 2026-11-18（首屏性能） | 按审查意见先建基线再优化：cold `shell_ready` p50 **848→371 ms**、warm **493→48 ms**；`boot_status` 快速路径、首屏骨架、页面级 lazy、`get_storage_health` 拆分、pool 缓存。setup 阶段实测 0.07/7.26 ms → **未改动** |
| 2026-11-18（首屏缺陷） | 修掉 READ-ONLY 数据跨 IPC 的**线格式不一致**：`Statistics`/`StorageHealth`/`ActivityDay`/`RepositoryEntry`/`Capabilities` 缺 `rename_all = "camelCase"`，导致 `undefined 天`、`schema vundefined`、`Invalid Date`，以及 **About 页白屏**；补 5 个线格式测试 + 页面自检 + `ui-pages` e2e（6 页 × 2 断言） |
| 2026-11-18（性能优化跟进，已并入 main） | Spool 计数改为定长缓冲读取；历史导入去掉未使用的提交主题与全量匹配引用副本；扩展统计改走 IndexedDB 索引。托盘扫描周期试过 100 ms，**实测 p95 437 ms（预算 87%）后回退 50 ms** |
| 2026-11-16（第一轮） | Phase 0 主体：环境勘察、隔离工具链、Monorepo、AGENTS/STATUS/spec 拆分/ADR、15 场景 Git 实验 + fixtures、WASM 验证、扩展构建 |
| 2026-11-18（Phase 4b 第十二轮） | 修掉 §22 的功能缺口：自定义成就**真正会被评估**（同事务、官方统计、复用内置记录路径）；新增 `user-achievements` e2e；顺带让 headless companion 也读规则 |
| 2026-11-18（Phase 4b 第十一轮） | 商店路线打通：MSIX 打包（真图标 / startupTask / 包内 bootstrapper）+ **helper 复制出包**，解决「hook 不能执行包内 exe」；`probe-packaged-app` 10 项实测通过 |
| 2026-11-18（Phase 4b 第十轮） | 依赖检查前移到**安装期**：`installer-hooks.nsh` 校验 WebView2 磁盘文件、内嵌官方 bootstrapper、失败退出码 2 且不留任何文件；`webviewInstallMode` 改为 skip 避免双份逻辑；Git 缺失只警告 |
| 2026-11-17（Phase 4b 第九轮） | 诊断导出补齐 §26.2 的「原始信息 + 明确警告」：`--export-diagnostics [--raw]` + 设置页入口，警告写进文件；WebView2 检测改为**校验磁盘文件**而非只读注册表（虚拟机已证实注册表会撒谎） |
| 2026-11-17（Phase 4b 第八轮） | 修「UI 白屏即可无从诊断」：加 `ui_error` 与全局错误上报（这次就是靠它定位到 TDZ 崩溃）；补无 WebView2 时的原生消息框兜底（`exit(2)`，CLI 豁免）；`webviewInstallMode` 由默认改为显式 |
| 2026-11-17（Phase 4b 第七轮） | 向导改为七步：新增依赖预检（WebView2 注册表 + Git）与浏览器扩展引导；`open_external` 只放行 http(s) 且不弹控制台；修掉「Repositories 页可装 hook」这句不实承诺 |
| 2026-11-17（Phase 4b 第六轮） | 多语言：英文原文作 key + 中文词典（178 条全覆盖），设置页语言选择、`system` 跟随系统；弹窗写死中文的问题一并修掉；新增 `check-i18n.mjs` 与 e2e 语言断言 |
| 2026-11-17（Phase 4b 第五轮） | 修「首启不走向导」：两个 builder 命令清单漂移（真实启动缺 `onboarding_status`/`install_hooks`），改为单一宏；`wizard` e2e 增加真实启动路径与重置流程（18 项）；新增设置页「Re-run setup」与 `--reset-onboarding` |
| 2026-11-17（Phase 4b 第四轮） | 新增「开机自启动」设置：写 `HKCU\...\Run`（卸载器同款名字）、启动时校准、改开关即时生效、`--autostart status/on/off`；判定逻辑测试通过，注册表写入受沙箱限制未验证 |
| 2026-11-17（Phase 4b 第三轮） | 修 release 无控制台启动即崩（日志重定向到 `companion.log`）、内置音效未打包；新增「启动时隐藏到托盘」设置（含配置导出兼容）；桌面启动器改为无终端窗口 |
| 2026-11-17（Phase 4b 第二轮） | §44.1 性能实测：hook p95 10.81 ms、commit→通知 p95 274/328 ms、空闲 CPU 0.12% 整机；修掉 500 ms 轮询导致的超预算（p95 1004 → 274）；顺带发现「自定义成就永不评估」并改记已知问题 |
| 2026-11-17（Phase 4b 第一轮） | 安装包打通（NSIS 工具集预置，签名产物 `.sig`）+ `installer` 真机 e2e；Tauri Updater 接入与数据库快照/回滚（6 个测试）；`release.yml` 发布流水线与工作流 linter |
| 2026-11-16（第二轮） | 补齐 §48.3 风险分析与 §48.5 实施计划；**解除 SQLite/SQLx 与 gix 两项阻塞**；ADR-0002 改为三方案实测；新增 `scripts/verify.mjs` 一键验证（42 测试全绿） |

---

## Phase 4a 任务表（自定义系统）

> 检查点 C 已由用户以「进入 phase4」批准。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 4a.1 | 资源安全校验（magic bytes / SVG / 配额） | `Tested` | 14 个测试，含伪装成 PNG 的脚本、SVG 执行向量 |
| 4a.2 | Popup Editor 约束与定位 | `Tested` | 15 个测试：范围钳制、四角定位、缩放 |
| 4a.3 | 主题包校验与导入 | `Tested` | 拒绝含脚本/样式/javascript: 的主题；未来 schema 拒绝而非猜测 |
| 4a.4 | Sound Manager | `Tested` | 导入音效 + 设置页可管理；`sound-manager` 7 项（含删除后降级不静音） |
| 4a.5 | Asset Manager | `Tested` | 导入/删除 + **接入弹窗渲染**；`asset-import` 9 项、`popup` 10 项 |
| 4a.6 | Popup Editor | `Tested` | 宽/缩放/角落/边距/时长/圆角/图标尺寸/透明度/动画/模糊 + 实时预览；`popup` 11 项 |
| 4a.7 | Event Filters 界面 | `Tested` | 设置页可编辑；9 项 e2e 含「不影响官方统计」 |
| 4a.8 | Achievement Builder（JSON 模式） | `Partially Implemented`（仅缺表单界面） | 定义校验与存储 12+12 项；**用户规则现已接入评估**：`ingest_commit_with_rules` 在同一事务内用官方统计判定并复用内置成就的记录路径；`user-achievements` e2e 6 项（真实 git 提交 → hook → helper → drain → 解锁）；**表单界面仍未做** |
| 4a.9 | 配置完整导入导出 | `Tested` | 11 个单元测试 + 18 项 e2e，含「失败不改变现状」 |

---

## Phase 4b 任务表（产品化与发布准备）

> 用户以「继续做phase4b」批准进入。详细过程见
> [phase-4b-progress.md](docs/development/phase-4b-progress.md)。

| # | 任务 | 状态 | 证据 / 缺口 |
| --- | --- | --- | --- |
| 4b.1 | Windows 安装包（NSIS） | `Tested` | 构建出安装包并签名；`scripts/e2e/installer.mjs` 真机装/跑/卸，含「卸载不删数据」 |
| 4b.2 | 更新机制（Tauri Updater，§40.3） | `Partially Implemented` | 插件+公钥+签名产物就位；数据库快照/回滚 6 个测试；**无发布 feed、设置页未接按钮、真实下载安装未验证** |
| 4b.3 | 发布流水线（§40.2） | `Platform Unverified` | `.github/workflows/release.yml` + 工作流 linter；本机无法执行 GitHub Actions，从未真正跑过 |
| 4b.4 | 平台签名（§40.4） | `Blocked` | 无 Authenticode 证书 → 安装包没有代码签名，SmartScreen 会警告；**更新签名（`.sig`）已做，两者不可互相替代** |
| 4b.17 | 界面数据正确性（§44.2） | `Tested` | 5 个线格式测试钉住 IPC 键名；页面自检把 `undefined`/`Invalid Date` 写进日志；`ui-pages` e2e 逐页打开真实窗口并断言「请求的页 = 渲染的页」且无占位文本（12 项） |
| 4b.16 | 微软商店（MSIX） | `Tested`（本机） | `scripts/msix/` 打真实 MSIX 并装机：安装/AOT 图标/startupTask/数据包外；**helper 复制到 `%LOCALAPPDATA%\bin` 后 hook 可执行并投递记录**（10 项探测全过）。当前用 bootstrapper 按需装 WebView2，认证备注见 `docs/development/store-certification-notes.md`；商店审核口径与无运行库真机流程**未验证**。见 §13–§14 |
| 4b.5 | 数据迁移（§26.4） | `In Progress` | 更新前后快照/还原路径已测；数据库迁移兼容旧版本尚未做 |
| 4b.6 | 多语言（§40.1） | `Tested` | 英文原文当 key + 简体中文字典（178/178）；`system` 跟随 webview locale；`UI LANGUAGE lang=… sample=…` 由 e2e 断言查表真的发生；`check-i18n.mjs` 守漏译 |
| 4b.7 | 性能优化（§40.1 / §44.1） | `Partially Implemented` | hook p95 10.81 ms、commit→通知 p95 274/328 ms、空闲 CPU 0.12% 整机；**首屏已实测并优化**：cold `shell_ready` p50 848→371 ms、warm 493→48 ms（各 12 次，分布不重叠，见 `docs/development/first-screen-performance.md`）。**未测**：首次 onboarding 路径、开窗口时的空闲 CPU、§44.3 并发/损坏场景 |
| 4b.8 | 错误恢复（§40.1） | `Partially Implemented` | 崩溃恢复、更新失败回滚已测；其余错误路径未逐一评估 |
| 4b.9 | 完整隐私设置（§40.1） | `Tested` | 隐私模式/托盘开关/日志脱敏；诊断导出默认脱敏、`--raw` 与设置页勾选才含真实仓库名，**警告写进文件本身**；`diagnostics-export` e2e 9 项 |
| 4b.10 | 启动行为：可选隐藏到托盘 | `Tested` | 设置页开关 + `startHidden`（默认 true）；`STARTUP hidden=` 日志对两种取值都有断言；配置导出对旧文件兼容 |
| 4b.11 | 首次运行向导可达性 | `Tested` | 根因：两个 builder 的命令清单漂移，`onboarding_status`/`install_hooks` 在真实启动里缺失；已收进单一宏 `app_commands!()`。`wizard` e2e 18 项，含**真实启动路径**断言与「重跑向导」 |
| 4b.15 | 安装期依赖安装（§25） | `Tested` | NSIS `PREINSTALL` 钩子：校验 `msedgewebview2.exe` 真实存在（不再信注册表）；缺 WebView2 用内嵌官方 bootstrapper 安装，仍失败则退出码 2 且不装任何文件；缺 Git 先走 `winget`、再下载固定版本官方安装器（`/CURRENTUSER`），失败仅警告；`DEVCH_FORCE_NO_*` 可分别强制两条分支 |
| 4b.14 | 无 WebView2 的原生兜底 | `Partially Implemented` | 建窗口前检查运行库，缺失则弹原生中英双语消息框（含下载入口）并 `exit(2)`；CLI 模式豁免；`--webview2-prompt` 可人工验证。**真正的缺运行库机器未验证** |
| 4b.13 | 启动前置检查 + 扩展引导（§25） | `Partially Implemented` | 向导七步，新增 `checks`（WebView2 注册表 + Git 双路径探测）与 `extension` 引导；`dependencies` e2e 5 项；**缺依赖时的提示文字本机无法验证** |
| 4b.12 | 开机自启动 | `Partially Implemented` | 默认关；写 `HKCU\...\Run\Devchievements`（与卸载器删除的名字一致）；判定逻辑 4 个测试 + `--autostart` 命令；**注册表写入在本环境的沙箱内被拒绝（WIN32_ERROR(5)），真机行为未验证** |

---

## 检查点 C

Phase 3a / 3b 的工作已完成为可验收状态，验收材料见
[checkpoint-c-acceptance.md](docs/development/checkpoint-c-acceptance.md)。

**状态：等待用户验收。** 按 §38，未获明确确认不进入 Phase 4a。

## 支持范围

**只支持 Windows**（10/11）。macOS 与 Linux 不在支持范围内，
这是产品决定（[ADR-0017](docs/decisions/0017-windows-only.md)），不是遗漏。

CI 只跑 `windows-latest`。规范 §29 的多平台要求据此收敛，
属经授权的偏离。

## 许可证

本项目以 **GPL-3.0-or-later** 发布，全文见 [LICENSE](LICENSE)。

`Cargo.toml` 与 `package.json` 的 `license` 字段与之一致。
