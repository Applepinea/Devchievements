# 架构决策记录（ADR）

本目录记录 Devchievements 的**重大技术决策**。

## 规则

1. **什么必须写 ADR**：影响两个及以上模块、难以回退、或规范明确要求出结论的决策。
2. **状态取值**：`Proposed` / `Accepted` / `Rejected` / `Superseded by ADR-XXXX` / `Partially Accepted`。
3. **不可篡改内容，但可修订并注明**：修订必须写清「改了什么、为什么改、新证据是什么」。
4. **必须带证据**：没有实测证据的结论要显式写「未验证 / Assume」，不得写得像已验证。
5. 新增 ADR 请复制 [0000-template.md](0000-template.md)，编号递增。

## Phase 0 必须有明确结论的 ADR（规范 §30.5）

| ADR | 主题 | 状态 | 一句话结论 |
| --- | --- | --- | --- |
| [0001](0001-wasm-strategy.md) | WASM 方案 | `Partially Accepted` | 编译与一致性通过；浏览器内加载未验证 |
| [0002](0002-hook-helper-implementation.md) | Hook 实现方案 | `Accepted` | **直读 .git**：p95 13.4 ms，优于 gix(66) 与 git 子进程(38.7) |
| [0003](0003-minimum-git-version.md) | 最低 Git 版本 | `Proposed` | **无结论**：只有 2.54.0 一个版本的数据 |
| [0004](0004-rewrite-classification-window.md) | 分类等待窗口 | `Accepted`（数值 Provisional） | 300 ms 不够（实测 436–452 ms），改 **800 ms** |
| [0005](0005-timezone-strategy.md) | 时区策略 | `Accepted`（选型） | 内置 TZDB + `jiff`；`local_date` 入库即冻结 |

## 其余 ADR

| ADR | 主题 | 状态 |
| --- | --- | --- |
| [0012](0012-isolated-dev-toolchain.md) | 本仓库隔离开发工具链（本仓库专属） | `Accepted` |
| [0013](0013-packfile-support.md) | Packfile 与 delta 解析（解除风险 A3） | `Accepted` |
| [0014](0014-desktop-shell-and-msvc.md) | 桌面外壳与 MSVC 工具链（解除 Phase 2a 构建阻塞） | `Accepted` |
| [0007](0007-ipc-and-spool.md) | IPC 与 Spool 传输设计 | `Accepted` |
| 0006 | 数据模型与投影设计 | 目标 Phase 1 |

| 0008 | Outbox 与通知所有权状态机 | 目标 Phase 1 |
| 0009 | Native Messaging 协议与版本协商 | 目标 Phase 3b |
| 0010 | 事件去重与 Rewrite 关联 | 目标 Phase 1 |
| 0011 | 浏览器侧 Engine 一致性 | 目标 Phase 3a |
