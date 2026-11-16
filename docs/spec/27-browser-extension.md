# 第 27 章　Browser Extension

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 27. Browser Extension

## 27.1 浏览器与技术

首发支持 Chrome、Microsoft Edge、Brave；采用 WXT + Manifest V3 + TypeScript。

## 27.2 页面活动识别

- 只识别可验证的 GitHub 活动；不得把页面访问、加载或按钮点击直接视为成功业务事件，必须确认操作已经成功。
- 可结合：网络响应观察、DOM 状态确认、可选 GitHub API 校验；不得把不稳定的 GitHub 内部 API 作为唯一事件来源。
- 必须取得唯一标识：Commit SHA、Release ID、Tag、PR ID 等。
- GitHub 是 SPA，必须处理路由变化、页面刷新、重复响应、加载失败、扩展暂停、用户切换账号。

## 27.3 安全桥接

MAIN world 注入必须使用受控消息桥，校验消息来源、结构、长度、类型和超时，不得信任页面提供的任意消息。

## 27.4 MV3 生命周期

Service Worker 可能随时被回收，状态必须持久化，不得依赖内存常驻。必须支持事件恢复、Native Messaging 重连、Outbox 恢复、Offer 恢复、ACK 重试。

## 27.5 三种运行状态

- **Connected**：Companion 是唯一权威。
- **Queued**：Companion 断线，扩展持久化事件但不自行解锁成就。
- **Standalone**：只有用户明确选择后才允许扩展独立运行；断线不得自动切换为 Standalone。

## 27.6 测试

Playwright + HTML Fixtures + 专用 GitHub 测试账号 + Canary 测试流程。不得使用用户真实账号进行未经确认的破坏性测试。

## 27.7 模式切换与数据迁移

**Browser Only 到 Full Experience**

- 扩展首次连接到 Companion，且本地 IndexedDB 中有数据时，询问用户是否导入（默认导入）。
- 一次性、幂等导入：扩展发送本地事件，保留 `event_id`、`external_id`、原始时间和可信度；Companion 按统一去重规则合并，来源标记为 `browser_standalone_import`。
- 导入静默，最多生成一条汇总通知；Companion 用自己的 Engine 从合并后的事件重算投影与成就，扩展本地的成就结果不作为权威。某项成就已在扩展本地解锁时，保留较早的解锁时间并标记来源。
- 导入可中断并续传，失败不影响 Browser Only 数据；导入完成后，扩展本地数据保留为只读备份，用户可清除。

**Full Experience 到 Standalone**

- 只有用户明确选择才进入 Standalone；扩展可以导入 Companion 导出的事件包，不自动同步。
- 回到 Connected 时，再次按上述规则幂等合并。

**测试**：重复导入不重复计数；中断后续传；大批量事件（上万条）；迁移期间到达新事件。

---
