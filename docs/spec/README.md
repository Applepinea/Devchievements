# 规范索引（Devchievements V2.5）

本目录是产品规范的拆分件。**正文与原文逐字一致**，仅按章切分以便引用与评审。
规范原文权威版本由用户持有（`Devchievements V2.5.md`）。

> ⚠️ 本目录是**只读参考**。任何对规范内容（统计口径、事件语义、通知规则、隐私策略）的改动
> 都必须先经用户确认（见 [46-user-confirmation-boundary.md](46-user-confirmation-boundary.md)）。

## 章节

| 文件 | 主题 |
| --- | --- |
| [00-agent-directives.md](00-agent-directives.md) | 给 Coding Agent 的最高优先级指令、阶段与检查点、最终交付标准 |
| [01-product-and-brand.md](01-product-and-brand.md) | 产品定位、品牌规范、与 GitHub 官方功能的区别、商标与视觉 |
| [02-modules-and-modes.md](02-modules-and-modes.md) | 核心模块、三种运行模式、数据来源能力矩阵 |
| [03-tech-stack-and-structure.md](03-tech-stack-and-structure.md) | 技术栈、Monorepo 结构、WASM 技术验证 |
| [04-core-engine.md](04-core-engine.md) | 事件处理流水线、Engine 设计、state_snapshot 读取范围 |
| [05-event-system.md](05-event-system.md) | 事件类型、统计投影、标准事件结构、来源可信度 |
| [06-repository-identity.md](06-repository-identity.md) | Canonical Repository Identity、Commit 去重 |
| [07-user-identity.md](07-user-identity.md) | Author/Committer 分离、多邮箱、Login 映射 |
| [08-git-activity-rules.md](08-git-activity-rules.md) | 默认统计规则、GitHub 合并、时间记录 |
| [09-git-hooks-and-capture.md](09-git-hooks-and-capture.md) | Hook 清单、安装/卸载、Amend 与 post-rewrite 时序、trace fixtures |
| [10-local-transport.md](10-local-transport.md) | IPC 优先 / Spool 回退、IPC 安全、Spool 能力与损坏处理 |
| [11-history-import.md](11-history-import.md) | 导入选项与要求 |
| [12-timezone-and-date-policy.md](12-timezone-and-date-policy.md) | IANA 时区、日界线、边界规则 |
| [13-permanent-achievements.md](13-permanent-achievements.md) | 永久成就性质、类别、版本化规则、回填策略、Hidden |
| [14-daily-rank.md](14-daily-rank.md) | 默认等级、规则、通知 |
| [15-event-filters.md](15-event-filters.md) | 可过滤类型与条件 |
| [16-outbox-and-notifications.md](16-outbox-and-notifications.md) | Outbox 原则、字段、新鲜度、默认行为、队列合并 |
| [17-notification-ownership.md](17-notification-ownership.md) | Offer/Claim/ACK 协议、状态表、转换、崩溃语义 |
| [18-native-messaging.md](18-native-messaging.md) | 通信结构、数据权威、版本协商、安全 |
| [19-20-21-sound-asset-popup.md](19-20-21-sound-asset-popup.md) | Sound Manager / Asset Manager / Popup Editor |
| [22-achievement-builder.md](22-achievement-builder.md) | JSON 规则、复杂度与确定性限制 |
| [23-theme-manager.md](23-theme-manager.md) | 主题包 |
| [24-desktop-ui.md](24-desktop-ui.md) | Dashboard、图鉴、统计、仓库管理、设置、诊断、空状态 |
| [25-onboarding-and-tray.md](25-onboarding-and-tray.md) | 安装向导、系统托盘、后台运行 |
| [26-privacy-and-data.md](26-privacy-and-data.md) | Local-first、Privacy Mode、Token、数据管理、存储配额 |
| [27-browser-extension.md](27-browser-extension.md) | 浏览器支持、页面识别、安全桥接、MV3 生命周期、三态、迁移 |
| [28-notification-visuals.md](28-notification-visuals.md) | 默认样式、稀有度时长、动画 |
| [29-platform-compatibility.md](29-platform-compatibility.md) | 降级链、Windows/macOS/Linux、全屏、兼容性矩阵 |
| [30-phase-0.md](30-phase-0.md) | Phase 0 初始化、技术验证、Git 实验、外部账户、验收 |
| [31-phase-1.md](31-phase-1.md) | Phase 1 实现内容、测试要求、验收 |
| [32-phase-2a.md](32-phase-2a.md) | Phase 2a 实现内容、真实测试、闭环演示、Crash Recovery 矩阵 |
| [33-checkpoint-a.md](33-checkpoint-a.md) | 人工检查点 A |
| [34-phase-2b.md](34-phase-2b.md) | Phase 2b 实现内容与验收 |
| [35-checkpoint-b.md](35-checkpoint-b.md) | 人工检查点 B |
| [36-phase-3a.md](36-phase-3a.md) | Phase 3a Browser Only |
| [37-phase-3b.md](37-phase-3b.md) | Phase 3b Native Messaging 与 Full Experience |
| [38-checkpoint-c.md](38-checkpoint-c.md) | 人工检查点 C |
| [39-phase-4a.md](39-phase-4a.md) | Phase 4a 自定义系统 |
| [40-phase-4b.md](40-phase-4b.md) | Phase 4b 产品化与发布准备 |
| [41-external-accounts.md](41-external-accounts.md) | 外部账户与密钥清单 |
| [42-store-listing.md](42-store-listing.md) | 商店发布描述 |
| [43-agents-status-adr.md](43-agents-status-adr.md) | AGENTS.md / STATUS.md / ADR 要求 |
| [44-testing-and-performance.md](44-testing-and-performance.md) | 性能目标、测试原则、可靠性测试、测量方法 |
| [45-development-flow.md](45-development-flow.md) | 每个 Phase 的 11 步开发执行流程 |
| [46-user-confirmation-boundary.md](46-user-confirmation-boundary.md) | 用户确认边界 |
| [47-final-checklist.md](47-final-checklist.md) | 最终交付验收清单 |
| [48-final-directive.md](48-final-directive.md) | 最终执行指令 |

## 完整性校验

拆分脚本按一级标题 `# <章号>. <标题>` 切分，并断言：

- 共 49 个一级章节（0–48），章号与出现顺序一一对应；
- 每个章节**恰好**被写入一个文件，无遗漏、无重复。

校验通过后才会落盘。
