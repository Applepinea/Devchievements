# 第 31 章　Phase 1：Core Engine 与数据层

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 31. Phase 1：Core Engine 与数据层

目标：完成独立于 UI 的核心业务逻辑。

## 31.1 实现内容

标准事件模型与验证；来源可信度；仓库身份识别；用户身份识别；Commit 去重与统计规则；时区系统；每日等级与里程碑；永久成就（含 `required_capabilities`）；用户过滤器；统计投影；Engine（含读集）；SQLite Storage Adapter 与 SQLx 迁移；事件持久化；Outbox 与通知所有权状态机（纯逻辑部分）；历史导入管线（引擎侧：分批、静默、使用 Commit 时间、幂等）；Golden Tests；IndexedDB Adapter 的接口设计（不展开浏览器协作）。

补充：Achievement Builder 规则的复杂度与确定性限制（嵌套深度、条件数、步数预算）；`retroactive_policy` 的四种取值；时区边界规则与时区属性测试。

## 31.2 测试要求

- 普通、Merge、Empty、Bot、非本人 Commit；Cherry-pick、Amend、Rebase；重复事件。
- 回放 Phase 0 产出的 Hook trace fixtures，验证 Amend / Rebase 在任意 Hook 到达顺序下结果一致。
- 时区变化、夏令时、跨日；历史导入与回填；多邮箱；多仓库。
- 每日等级、永久成就、固定口径不受用户过滤器影响。
- 通知所有权状态机属性测试（见「通知所有权协议」测试要求）。
- 事件重放、崩溃恢复、Outbox 事务原子性。

## 31.3 验收

Engine 单元测试与 Golden Tests 通过；统计口径文档完成；数据模型明确；Outbox 事务机制完成；历史重放默认静默；初步性能基准完成。Native 与 WASM（若采用）结果一致。

---
