# 第 43 章　AGENTS.md、STATUS.md 与 ADR

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 43. AGENTS.md、STATUS.md 与 ADR

## 43.1 AGENTS.md

根目录必须创建，至少包括：产品定位、项目结构、技术栈、开发与测试命令、代码风格、模块边界、安全约束、隐私要求、Phase 顺序与人工检查点、验收要求。子目录可有自己的 AGENTS.md，但不得与根规范冲突。

## 43.2 STATUS.md

项目唯一主要进度状态表。状态取值：Not Started、In Progress、Implemented、Tested、Partially Implemented、Platform Unverified、Blocked。

必须区分：代码已编写、构建已通过、自动化测试已通过、实际运行测试已通过、平台兼容性已验证。不能把 Implemented 当成 Tested，不能把构建成功当成真实运行成功。

## 43.3 ADR

重大技术决策记录在 `docs/decisions/`，至少包括：WASM 方案；数据模型；Git Hook 策略与 Helper 实现方案；Amend / Rebase 处理与分类等待窗口；最低支持的 Git 版本；IPC 与 Spool 设计；Outbox 与通知所有权状态机；Native Messaging 协议；时区策略。

---
