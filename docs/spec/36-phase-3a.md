# 第 36 章　Phase 3a：Browser Only

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 36. Phase 3a：Browser Only

本阶段只实现浏览器独立运行能力。

## 36.1 实现内容

WXT 扩展（MV3）；GitHub 活动识别（Commit、Release，随后 PR、Issue）；Browser Only Engine（WASM 或 TypeScript，按 Phase 0 决策）；IndexedDB 与独立 Outbox；浏览器通知与音效；浏览器端设置、Dashboard 与图鉴（受能力限制）；用户身份映射（GitHub Login 到 Git Email）。

## 36.2 验收

扩展可独立安装；GitHub 事件识别与验证正确；IndexedDB 持久化正确；成就解锁与浏览器通知正确；Service Worker 回收后状态可恢复；Golden Tests 通过；不依赖 Native Messaging。

---
