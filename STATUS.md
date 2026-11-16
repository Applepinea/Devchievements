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
PASS  test                  4467 ms   147 tests passed, 0 failed
PASS  build                    3982 ms
PASS  build-wasm                396 ms
PASS  extension-build         10371 ms
ALL CHECKS PASSED
```

原始日志与机器可读摘要：`.verify/`

---

## 当前阶段

**Phase 0：项目基础与技术验证** —— `Partially Implemented`（已完成，见下）
**Phase 1：Core Engine 与数据层** —— `In Progress`

**下一个门禁：检查点 A**（需 Phase 2a 完成）。当前**没有任何检查点被跨越。**

---

## Phase 1 任务表

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1.1 | 标准事件模型与验证 | `Tested` | `crates/events`，31 个测试 |
| 1.2 | 来源可信度 | `Tested` | 含「unverified 不计入官方统计」断言 |
| 1.3 | 仓库身份识别 | `Tested` | SSH/HTTPS/git 后缀/大小写/跨平台不合并 |
| 1.4 | Commit 去重键 | `Tested` | `repo_id + sha`；patch id 仅辅助 |
| 1.5 | 用户身份识别（多邮箱） | `Not Started` | 需 Git 配置读取 |
| 1.6 | 时区系统 + 日界线 | `Tested` | 内置 TZDB；DST 与 04:00 边界已测 |
| 1.7 | 每日等级与每日里程碑 | `Tested` | 14 个测试；用户阈值不影响官方口径 |
| 1.8 | 永久成就（含 required_capabilities） | `Tested` | 13 个测试；能力由条件推导 |
| 1.9 | 用户过滤器 | `Not Started` | 独立模块，尚未实现 |
| 1.10 | 统计投影 | `Partially Implemented` | 类型与读集已定；持久化读写待接 |
| 1.11 | Engine（含读集） | `Tested` | 7 个测试；读集有断言守护 |
| 1.12 | SQLite Storage Adapter 与 SQLx 迁移 | `Partially Implemented` | 建表/往返/回滚/级联/重开已测；迁移版本化待做 |
| 1.13 | 事件持久化 | `Partially Implemented` | 表结构可用；Engine 输出尚未接入事务 |
| 1.14 | Outbox 与通知所有权状态机（纯逻辑） | `Tested` | 18 个测试；合法转换表 + 防重复展示 |
| 1.15 | 历史导入管线（引擎侧） | `Not Started` | 依赖投影落地 |
| 1.16 | Golden Tests | `Tested` | 16 个测试回放 15 份真机 trace |
| 1.17 | 时区属性测试 | `Not Started` | 已有逐例测试，缺系统性属性测试 |
| 1.18 | packfile 解析 | `Not Started` | **风险最高**，见 risk-analysis A3 |

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
