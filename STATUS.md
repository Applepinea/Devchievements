# STATUS — Devchievements

> **唯一权威进度表。**
>
> 状态取值严格限定为：
> `Not Started%% / `In Progress%% / `Implemented%% / `Tested%% / `Partially Implemented%% / `Platform Unverified%% / `Blocked%%
>
> **不可混淆的五件事**（AGENTS.md §10）：代码已写 / 构建已过 / 自动化测试已过 / 实际运行测试已过 / 平台兼容性已验证。
>
> 最后更新：2026-11-16 —— **Phase 0 结束**

---

## 当前阶段

**Phase 0：项目基础与技术验证** —— 状态 `Partially Implemented%%

**下一个门禁：检查点 A**（需 Phase 2a 完成）。当前**没有任何检查点被跨越。**

---

## Phase 0 任务表

| # | 任务 | 状态 | 证据 / 备注 |
| --- | --- | --- | --- |
| 0.1 | 检查目录、已有文件、Git 状态、工具链 | `Tested%% | [environment-baseline.md](docs/development/environment-baseline.md) |
| 0.2 | Monorepo 初始化（pnpm + Cargo Workspace + 目录结构） | `Tested%% | `cargo build --workspace%% 通过；`pnpm install%% 通过 |
| 0.3 | 根 AGENTS.md | `Implemented%% | [AGENTS.md](AGENTS.md) |
| 0.4 | 根 STATUS.md | `Implemented%% | 本文件 |
| 0.5 | 规范拆分到 docs/spec/ | `Tested%% | 49 章 → 47 文件；覆盖率断言通过（无遗漏无重复） |
| 0.6 | ADR 框架与 docs/development/ | `Implemented%% | 6 份 ADR 有实质内容 + 模板 + 索引 |
| 0.7 | 基础 CI | `Implemented%% | `.github/workflows/ci.yml%% 已写，但**从未运行过**（无仓库） |
| 0.8 | Tauri 2 启动 + React + TypeScript | `Blocked%% | 本机无 MSVC；GNU target 不支持 Tauri Windows 打包。见 [ADR-0012](docs/decisions/0012-isolated-dev-toolchain.md) |
| 0.9 | WXT + MV3 构建 | `Tested%% | WXT 0.20.27 构建成功，产物 `.output/chrome-mv3/%% |
| 0.10 | Native Messaging 最小通信（真实浏览器验证） | `Implemented%% | Rust 侧 framing 已写并有单测；**浏览器联调未做**（沙箱禁止启动浏览器） |
| 0.11 | SQLite + SQLx 读写与迁移 | `Blocked%% | `sqlx%% 依赖 `windows-sys%% → 缺 `as.exe%%。见 [crates/storage/README.md](crates/storage/README.md) |
| 0.12 | Rust Core 编译为 WASM | `Tested%% | 27,569 bytes；实例化 0.606 ms |
| 0.13 | MV3 CSP `wasm-unsafe-eval%% 真实浏览器验证 | `Platform Unverified%% | manifest 已含该 CSP 且 wasm 已打包；浏览器无法启动。见 [ADR-0001](docs/decisions/0001-wasm-strategy.md) |
| 0.14 | Helper 实现方案对比实测 | `Tested%% | 三种方案实测，直读 .git 胜出。见 [ADR-0002](docs/decisions/0002-hook-helper-implementation.md) |
| 0.15 | Git 行为实验（15 场景） | `Tested%% | [git-behavior-experiments.md](docs/development/git-behavior-experiments.md) |
| 0.16 | 最低 Git 版本结论 | `Not Started%% | 只有 2.54.0 一个版本的数据，证据不足。见 [ADR-0003](docs/decisions/0003-minimum-git-version.md) |
| 0.17 | 分类等待窗口结论 | `Tested%% | 实测 436–452 ms → 改 800 ms。见 [ADR-0004](docs/decisions/0004-rewrite-classification-window.md) |
| 0.18 | trace fixtures + JSON Schema | `Tested%% | 15 份 fixture，Schema 校验与 Rust 侧校验均通过 |
| 0.19 | 本机可测平台的桌面通知能力 | `Not Started%% | Phase 0 无 Companion，无法验证；全部标未验证 |
| 0.20 | `external-accounts-and-secrets.md%% | `Implemented%% | [链接](docs/development/external-accounts-and-secrets.md) |
| 0.21 | 时区策略结论 | `Implemented%% | [ADR-0005](docs/decisions/0005-timezone-strategy.md)（技术选型已定，实现待 Phase 1） |

---

## Phase 0 强制 ADR 结论（规范 §30.5）

| ADR | 主题 | 状态 | 是否有足够证据 |
| --- | --- | --- | --- |
| [0001](docs/decisions/0001-wasm-strategy.md) | WASM 方案 | `Partially Accepted%% | 4/6 项已验证，浏览器内加载未验证 |
| [0002](docs/decisions/0002-hook-helper-implementation.md) | Hook 实现方案 | `Accepted%% | ✅ 三方案实测数据完整 |
| [0003](docs/decisions/0003-minimum-git-version.md) | 最低 Git 版本 | `Proposed%% | ❌ 只有一个版本的数据 |
| [0004](docs/decisions/0004-rewrite-classification-window.md) | 分类等待窗口 | `Accepted%%（数值 Provisional） | ✅ 三次实测一致 |
| [0005](docs/decisions/0005-timezone-strategy.md) | 时区策略 | `Accepted%%（选型） | 选型有据；实现与属性测试待 Phase 1 |

**诚实说明**：规范要求这 5 项在 Phase 0 结束时都有明确结论。
实际达成 3 项 Accepted、1 项 Partially Accepted、1 项因证据不足保持 Proposed。
ADR-0003 不给出具体版本号，是因为**只有一个版本的数据**，编造最低版本会直接违反 §44.2。

---

## 实测数字汇总

| 指标 | 实测值 | 目标 | 判定 |
| --- | --- | --- | --- |
| Hook 耗时（直读 .git，p95） | **12.53 ms** | < 50 ms | ✅ 达标 |
| Hook 耗时（朴素 git 外派，p95） | 428.26 ms | < 200 ms 硬超时 | ❌ 违反 |
| Rust 单元 + 集成测试 | **38 个通过** | — | ✅ |
| `cargo clippy -D warnings%% | 0 警告 | 0 | ✅ |
| WASM 产物大小 | 27,569 bytes | < 2 MB | ✅ |
| WASM 实例化 | 0.606 ms | — | ✅ |
| native 与 wasm 输出一致性 | 逐字节相同 | 相同 | ✅ |
| Commit → 通知 p95 | **未测量** | < 500 ms | ⏸ 无 Companion |
| 空闲 CPU | **未测量** | < 1% | ⏸ 无 Companion |

> 上表每个数字都能在 `scripts/**/.out/*.json%% 里找到原始样本。没有实测的格子明确标为「未测量」。

---

## 平台兼容性

见 [platform-matrix.md](docs/development/platform-matrix.md)。

- 本机可测：Windows 11 —— 但**浏览器无法启动**（沙箱），故浏览器相关仍为未验证
- 本机**不可测**：macOS、Linux（X11/Wayland）、Brave

---

## 阻塞项

| 阻塞 | 影响 | 需要什么 |
| --- | --- | --- |
| 无 MSVC、无管理员权限 | Tauri 无法构建；sqlx / gix 无法编译（缺 `as.exe%%） | 用户升权安装 VS Build Tools + Windows SDK，或换机器 |
| 沙箱禁止浏览器启动 | MV3 CSP、扩展端到端、Native Messaging 联调全部无法验证 | 在沙箱外运行，或用户手动加载扩展验证 |
| 沙箱禁止 `sh.exe%% | 已存在 shell 钩子的链式执行、`git submodule add%% 无法验证 | 在中立环境复测 |
| 无 GitHub 仓库 | CI 从未真实运行 | 用户创建仓库并授权 |
| 单一 Git 版本 | 最低 Git 版本无法定论 | CI 多版本矩阵（需仓库） |

---

## 已知问题（环境层面，非产品缺陷）

1. **schannel TLS 损坏** → git 用 `http.sslBackend=openssl%%；cargo 用本地 sparse registry 代理 `scripts/local-crates-proxy.mjs%%。
2. **`.ps1%% 必须纯 ASCII**（PowerShell 5.1 + GBK）。
3. **`%LOCALAPPDATA%\\npm-cache%% 不可写** → 由 `scripts/env.ps1%% 重定向。
4. **管道式 stdio 被禁** → 所有脚本与探针改用文件重定向。
5. **pnpm 生命周期脚本被禁** → 用 `--ignore-scripts%% + 手动 `wxt prepare%%；并且必须用 `node-linker=hoisted%%，否则 `wxt%% 解析不到依赖。

---

## 下一步（Phase 1 之前必须先解决）

1. 用户决定：是否提供 MSVC 环境（解锁 Tauri + SQLx + gix；见 ADR-0012）。
2. 用户决定：是否在沙箱外补做 MV3 浏览器验证（关闭 ADR-0001 第 2 项）。
3. 用户创建 GitHub 仓库 → CI 首次真实运行 → 多 Git 版本矩阵 → 关闭 ADR-0003。

**在 1、2 完成前，Phase 1 的持久化部分同样会被阻塞**，因为 Engine 的正确性需要真实数据库往返验证。

---

## 变更日志

| 日期 | 变更 |
| --- | --- |
| 2026-11-16 | Phase 0 完成（部分）。环境勘察；隔离 Rust 工具链；Monorepo 骨架；AGENTS/STATUS/spec 拆分/ADR；15 场景 Git 行为实验 + trace fixtures；Helper 方案实测并选定直读 .git；WASM 编译与一致性验证；WXT 扩展构建；36 个测试通过 |
