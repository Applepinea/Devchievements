# 第 37 章　Phase 3b：Native Messaging 与 Full Experience

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 37. Phase 3b：Native Messaging 与 Full Experience

## 37.1 实现内容

Native Messaging Host；协议版本协商；Companion 连接管理；Connected / Queued / Standalone 状态机；事件补交与断线恢复；通知所有权协议（Offer / Claim / ACK）与 Outbox 状态恢复；MV3 Service Worker 重连；新鲜度窗口；通知路由；浏览器与桌面通知防重复；扩展安装与配置引导；Chrome、Edge、Brave 基础兼容测试。

补充：Browser Only 到 Full Experience 的数据迁移（见「模式切换与数据迁移」）。

## 37.2 关键测试

正常连接；Companion 关闭与重启；浏览器重启；Service Worker 回收；端口断开；ACK 丢失；Claim 竞争；Offer 过期；迟到 Claim；多浏览器或多 profile 同时连接；重复事件；离线期间产生事件；多条通知同时等待；协议不兼容。

Service Worker 回收必须至少覆盖三个时点：Offer 发出后、Claim 之后、ACK 之前。数据迁移测试：重复导入、中断续传、大批量事件、迁移期间到达的新事件。

## 37.3 验收

Connected、Queued 正常；Standalone 仅在用户明确选择后启用；Companion 始终是 Full Experience 唯一权威；通知不因竞争重复展示；离线事件可恢复；重连不产生过期庆祝通知；协议不兼容时明确报错。

数据迁移幂等；迁移后 Companion 的统计与成就与 Engine 重算结果一致。

---
