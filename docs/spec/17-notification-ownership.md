# 第 17 章　通知所有权协议

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 17. 通知所有权协议

Full Experience 的正式通知交付协议，目标是确保浏览器扩展与桌面 Companion 不会重复展示同一条通知。Desktop Only 与 Browser Only 使用同一状态机的子集。

## 17.1 基本流程

```text
Companion 创建 Offer（notification_id, event_id, deadline）
   → 已连接的扩展检查页面可见性
   → 可见：Claim；不可见：立即回复 Decline
   → Claim 被授予后才展示，展示成功后发 ACK
   → Companion 更新 Outbox 状态
   → 全部 Decline / 超时 / 无已连接扩展：降级桌面通知
```

## 17.2 状态表

| 状态 | 含义 | 终态 |
| --- | --- | --- |
| `created` | Outbox 记录已创建 | 否 |
| `offered` | 已向已连接扩展发出 Offer | 否 |
| `claimed` | 某个扩展实例已原子认领 | 否 |
| `displayed` | 扩展 ACK：已成功展示 | 否 |
| `declined` | 所有已连接扩展明确拒绝 | 否 |
| `expired` | Offer 到期无人认领 | 否 |
| `desktop_pending` | 等待桌面展示 | 否 |
| `desktop_displayed` | 桌面已展示 | 否 |
| `completed` | 展示与音效全部完成 | 是 |
| `delivery_unknown` | 已认领但 ACK 丢失，且扩展无法联系 | 是 |
| `failed` | 重试耗尽 | 是 |
| `suppressed` | 按策略静默 | 是 |

## 17.3 合法状态转换

- `created` → `offered`（Full Experience 且存在已连接扩展）。
- `created` → `desktop_pending`（Desktop Only，或没有已连接扩展）。
- `created` → `suppressed`（新鲜度或静默策略）。
- `offered` → `claimed`（首个有效 Claim，原子比较并交换）。
- `offered` → `declined`（全部明确拒绝）；`offered` → `expired`（deadline 到期）。
- `declined` / `expired` → `desktop_pending`。
- `claimed` → `displayed`（收到 ACK）；`displayed` → `completed`。
- `claimed` → `desktop_pending`（仅当扩展明确报告「未能展示」）。
- `claimed` → `delivery_unknown`（ACK 超时，状态查询重试后扩展仍不可达）。
- `desktop_pending` → `desktop_displayed` → `completed`。
- 任意非终态 → `failed`（重试耗尽）。

## 17.4 唯一认领与竞争

- Claim 必须通过原子状态转换实现，不得依靠前端布尔值。
- 扩展只有在收到 Claim 被授予的响应后才可展示。
- 迟到的 Claim 在通知进入 `desktop_pending` 之后一律以 `already_handled` 拒绝。
- 多个浏览器或 profile 同时连接时，所有实例都收到 Offer，first-wins，其余收到 `already_claimed`。

## 17.5 ACK 语义与丢失

- 只有确认通知已成功展示，才能 ACK；ACK 不代表所有副作用都已完成。
- ACK 默认 5 秒超时后，Companion 发起幂等状态查询（最多 3 次）：扩展确认已展示则进入 `completed`；明确报告未展示则回退桌面；仍不可达则进入 `delivery_unknown`。
- `delivery_unknown` 不回退桌面通知（宁可少通知，不重复通知）；对应成就必须在图鉴中以「新解锁」标记保证可发现。

## 17.6 延迟预算

- Offer deadline 默认 150 ms（可配置），计入 Commit 到通知 p95 < 500 ms 的预算。
- 扩展不可见或不可用时必须立即回复 Decline，不得让 Companion 等到超时；超时只用于连接异常。
- 没有已连接扩展时不创建 Offer，直接走桌面。

## 17.7 MV3 Service Worker 回收

必须支持 Native Messaging 端口重建、待处理 Offer 恢复、通知状态查询、幂等 Claim、ACK 重试和重连后状态同步，不能依赖 Service Worker 常驻。

## 17.8 测试要求

- 状态机属性测试：任意消息序列（含重复、乱序、丢失）下，每条通知最多进入一个展示终态；每条 `created` 记录最终都会到达终态；重放同一消息序列结果幂等。
- 场景测试：正常 Claim、全部 Decline、Offer 过期、迟到 Claim、Claim 竞争、ACK 丢失、扩展崩溃、Companion 重启、多浏览器同时连接。

## 17.9 桌面展示的崩溃语义

桌面渠道必须在真正展示之前持久化「展示尝试」（`display_attempted_at`）。崩溃恢复时，对已有展示尝试但未完成的通知，不得重复展示完整弹窗，而是标记为 `delivery_unknown`，并保证图鉴中的新解锁标记可见。原则：宁可少通知，不重复通知。

---
