# 第 2 章　功能总览与运行模式

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 2. 功能总览与运行模式

## 2.1 核心模块

1. **Core Engine**：事件处理、统计投影、成就判定。
2. **Achievement System**：永久成就、每日等级、每日里程碑。
3. **Activity System**：Git 活动、仓库、趋势与历史记录。
4. **Notification System**：通知、音效、图片和弹窗编辑。
5. **Desktop Companion**：桌面主程序、系统托盘、数据库与 Git Hooks。
6. **Browser Extension**：GitHub 活动识别，Browser Only 与 Full Experience。

## 2.2 三种运行模式

**Browser Only**：完全独立运行的浏览器扩展。使用 IndexedDB、独立 Engine、独立 Outbox 和独立通知系统；不依赖 Companion；追踪浏览器中可验证的 GitHub 活动。

**Desktop Only**：完全独立运行的桌面程序。使用 SQLite；通过本地 Git Hooks 追踪活动；使用桌面通知和音效；不依赖浏览器扩展。

**Full Experience**：扩展负责识别 GitHub 活动，Native Messaging 负责跨进程通信；Companion 是唯一数据权威、唯一 SQLite 写入者和最终成就仲裁者。扩展断线时可持久化排队，重连后去重、补交并同步状态。Full Experience 下，扩展与 Companion 不得对同一事件分别独立解锁成就。

## 2.3 数据来源能力（Source Capability）

不同运行模式能观察到的事件不同，产品必须诚实地反映这一点：

| 事件 | 本地 Git（Desktop Only） | 浏览器扩展（Browser Only / Full） |
| --- | --- | --- |
| Commit | 支持（post-commit） | 支持（GitHub 网页 Commit） |
| Release | 不支持 | 支持 |
| PR Opened / Merged | 不支持 | 支持 |
| Issue Opened / Closed | 不支持 | 支持 |
| Repository Created | 不支持 | 支持 |
| Star Given | 不支持 | 支持 |
| Local Merge | 支持（post-merge，单独分类，不等于 PR Merge） | 不支持 |
| Star Received | 不支持 | 不支持（P2，需要 GitHub API 授权） |

规则：

- 每个成就、统计指标和 UI 面板都必须声明 `required_capabilities`。
- 当前模式不具备所需能力时，UI 显示「需要浏览器扩展」之类的明确状态，不得把永远为零的数据当成真实数据展示。

---
