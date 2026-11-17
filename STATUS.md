# STATUS — Devchievements

> **唯一权威进度表。**
>
> 状态取值严格限定为：
> `Not Started` / `In Progress` / `Implemented` / `Tested` / `Partially Implemented` / `Platform Unverified` / `Blocked`
>
> **不可混淆的五件事**（AGENTS.md §10）：代码已写 / 构建已过 / 自动化测试已过 / 实际运行测试已过 / 平台兼容性已验证。
>
> 最后更新：2026-11-16 —— **Phase 0 结束（第二轮：回填遗漏交付物 + 解除两项阻塞）**

---

## 唯一验证命令

```powershell
node scripts/verify.mjs
```

当前结果（真实执行，非推测）：

```text
PASS  fmt                       174 ms
PASS  clippy                    822 ms
PASS  test                ~11.7 s   237 tests passed, 0 failed
PASS  build                    3982 ms
PASS  build-wasm                396 ms
PASS  extension-build         10371 ms
ALL CHECKS PASSED
```

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
| 3a.7 | 身份映射 GitHub Login → Git Email | `Not Started` | |
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
| Commit → 通知 p95 | **未测量** | < 500 ms | ⏸ 无 Companion |
| 空闲 CPU | **未测量** | < 1% | ⏸ 无 Companion |

> 每个数字都能在 `.verify/` 或 `scripts/**/.out/*.json` 找到原始样本。未实测的格子明确标注「未测量」。

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
| 2026-11-16（第一轮） | Phase 0 主体：环境勘察、隔离工具链、Monorepo、AGENTS/STATUS/spec 拆分/ADR、15 场景 Git 实验 + fixtures、WASM 验证、扩展构建 |
| 2026-11-16（第二轮） | 补齐 §48.3 风险分析与 §48.5 实施计划；**解除 SQLite/SQLx 与 gix 两项阻塞**；ADR-0002 改为三方案实测；新增 `scripts/verify.mjs` 一键验证（42 测试全绿） |

---

## 支持范围

**只支持 Windows**（10/11）。macOS 与 Linux 不在支持范围内，
这是产品决定（[ADR-0017](docs/decisions/0017-windows-only.md)），不是遗漏。

CI 只跑 `windows-latest`。规范 §29 的多平台要求据此收敛，
属经授权的偏离。

## 许可证

本项目以 **GPL-3.0-or-later** 发布，全文见 [LICENSE](LICENSE)。

`Cargo.toml` 与 `package.json` 的 `license` 字段与之一致。
