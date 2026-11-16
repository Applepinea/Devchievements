# 第 4 章　Core Engine 架构

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 4. Core Engine 架构

## 4.1 事件处理流水线

```text
Event Detection → Normalization → Validation → Deduplication → Persistence
→ Filtering → Projection Update → Achievement Evaluation
→ Transactional Outbox → Consumer → Notification / Sound / UI
```

事件、统计投影和成就解锁结果分开存储；原始事件不等于统计结果；通知等外部副作用不得进入 Engine 的计算流程。

## 4.2 Engine 设计

Engine 是纯逻辑层：

```text
Engine(state_snapshot, event, injected_context)
  -> { state_changes, side_effect_intents }
```

Engine 不得直接访问系统时钟、随机 ID、数据库、操作系统通知、网络和文件系统。外部依赖通过注入接口提供：Clock Provider、Timezone Provider（IANA）、ID Generator、Configuration Provider。所有依赖必须可在 Golden Tests 中替换。

## 4.3 state\_snapshot 读取范围

- 允许包含：当前仓库必要投影；当前统计周期的每日统计；与事件类型相关的累计统计；规则判断所需的成就状态；当前适用的配置版本。
- 每条规则必须声明读集（read-set），Adapter 只加载读集内的投影。
- 不得每次 Commit 都扫描所有历史原始事件或全部仓库的统计数据；使用增量投影、数据库索引、必要缓存和版本化快照。
- 事务语义：Engine 输出的状态变更与 Outbox 意图，由 Storage Adapter 在一个事务内原子应用（SQLite 事务 / IndexedDB 事务）。

---
