# 第 0 章　给 Coding Agent 的最高优先级指令

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 0. 给 Coding Agent 的最高优先级指令

你将负责从零开始设计、实现、测试和交付 **Devchievements**：一个跨平台桌面软件及浏览器扩展。

Devchievements 是本地优先的开发活动追踪与成就系统，把开发者的 Git 活动转化为游戏化成就、每日等级、提示音和可自定义的视觉通知。它不是简单的 Git 统计工具，也不是单纯的通知插件，而是具有完整 Engine、事件系统、成就系统、持久化机制、桌面 UI、浏览器扩展和跨进程通信能力的产品。

## 0.1 核心执行原则

1. 先完整理解规范，再开始实现。
2. 先检查现有目录、Git 状态、开发环境和工具链；不得未经检查覆盖已有文件。
3. 不得为了快速交付而跳过核心架构。
4. 不得把尚未实现的功能标记为已完成；不得把仅通过编译的功能标记为已通过实际运行测试。
5. 不得自行创建付费账户、购买证书或域名、使用未经授权的密钥，或代用户接受法律协议。
6. 不得擅自改变统计口径、事件语义、通知规则或隐私策略。
7. 重大架构变更、不可逆数据操作、外部账户与密钥操作、跳过人工检查点，必须先征求用户确认（完整清单见「用户确认边界」一章）。
8. 普通实现细节由 Agent 自主决定，但重要决策必须写入 ADR。
9. 当前环境无法验证的平台或能力，一律标记为 Platform Unverified，不得假装已验证。

## 0.2 阶段与人工检查点

严格按以下顺序推进：

```text
Phase 0 → Phase 1 → Phase 2a → 检查点 A → Phase 2b → 检查点 B
→ Phase 3a → Phase 3b → 检查点 C → Phase 4a → Phase 4b
```

- **检查点 A（v0.1-alpha）**：Phase 2a 完成后，展示本地 Git 到弹窗和提示音的真实闭环。
- **检查点 B（v0.1）**：Phase 2b 完成后，展示完整的 Desktop Only 产品。
- **检查点 C**：Phase 3b 完成后，展示 Browser Only、Desktop Only 与 Full Experience。
- 没有用户明确确认，不得跨越任何检查点。
- 用户确认检查点 B 之前，不得开始浏览器扩展开发。
- Phase 3a 与 Phase 3b 必须分别验收；Browser Only 完成不等于 Full Experience 完成。
- 等待确认期间，可以继续推进不依赖该决策的文档、测试和修复工作。

## 0.3 最终交付标准

- 完整 Monorepo、Rust Core Engine、Desktop Companion、Rust Hook Helper。
- Git Hooks、本地 IPC 与 Spool、SQLite 持久化。
- 成就、每日等级与每日里程碑系统；通知、音效与 Outbox。
- 完整桌面 UI、Browser Extension、Native Messaging Host。
- Browser Only、Desktop Only、Full Experience 三种模式。
- 用户自定义成就、声音、图片、主题和通知外观。
- 自动化测试与 Golden Tests、CI/CD、安装 / 升级 / 卸载机制。
- 开发、架构、兼容性和发布文档。

---
