# 第 24 章　Desktop Companion UI

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 24. Desktop Companion UI

桌面主界面是正式产品的重要组成部分，不能只交付托盘图标和弹窗。所有页面必须从真实数据库读取数据，不得使用静态假数据冒充完成状态。

## 24.1 Dashboard

展示：总 Commit 数量、当前每日等级与等级进度、最近解锁成就、连续活跃天数、最近活动时间线。

- Release / PR / Issue 数量依赖浏览器事件。Desktop Only 下这些面板显示「需要浏览器扩展」，不显示永远为零的数字；Browser Only 与 Full Experience 下才显示数据。
- **统计口径标注**：Dashboard 的总数受用户过滤器影响，标注「统计口径：用户过滤设置」；成就进度使用固定官方口径，标注「固定官方口径」。两者数字可能不同，界面上必须让用户看得出原因。

## 24.2 Achievement Gallery

- 支持：全部 / 已解锁 / 未解锁 / 隐藏成就；按类别和稀有度筛选；解锁时间、解锁条件、成就详情。
- 依赖当前模式不可用能力的成就，显示「需要浏览器扩展」状态，不显示成可达成的进度。
- **新解锁标记**：必须记录并显示解锁时间、解锁来源、是否为新解锁、是否已查看。无论成就是实时解锁、延迟通知、历史回填，还是超过 24 小时后静默解锁，或通知状态为 `delivery_unknown`，都必须在图鉴中有可见入口。用户打开图鉴后可清除新解锁标记，并可一键全部标记为已读。
- 不得仅依靠弹窗作为成就发现入口。

## 24.3 Activity & Statistics

每日活动日历、Commit 历史趋势、仓库活动统计、每日等级历史；PR、Release、Issue 统计（受能力限制，规则同上）。

## 24.4 Repository Manager

仓库列表、识别状态、过滤与统计、启用与排除设置、历史导入入口（见「历史导入」）、Hook 安装状态。

## 24.5 Settings

通知、音效、图片、隐私、运行模式、Git 身份（多邮箱）、时区与日期、数据管理、自定义成就、主题。自定义类设置随 Phase 4a 逐步开放。

## 24.6 About & Diagnostics

软件版本、运行日志、数据库状态、Git Hook 状态、Companion / IPC / Spool 状态、Native Messaging 与扩展连接状态、Outbox 状态分布（含 `delivery_unknown` 列表）、Rewrite 分类诊断日志、诊断信息导出。

此外：Offer / Claim 延迟分布（p50 / p95）；最近 N 条 `delivery_unknown` 的原因码（如 `ack_timeout`、`extension_unreachable`、`claim_rejected`）；存储健康（数据库大小、Spool 占用、自定义资源占用、IndexedDB 配额使用），接近阈值时主动提示，不自动删除用户数据。

## 24.7 Phase 2a 最小诊断视图

Phase 2a 只要求一个最小诊断视图：最近事件、最近解锁、Outbox 状态、Hook 与 IPC 状态。它用于让检查点 A 的闭环可被验证，完整页面在 Phase 2b 交付。

## 24.8 空状态与引导

Dashboard、图鉴和统计页必须设计无数据状态：已配置邮箱但长期没有 Commit 时，引导检查 Hook 状态与邮箱配置，并提供「发送测试事件」；Hook 未安装时给出安装入口；当前模式缺少浏览器能力时，提示安装扩展。文案不得责备用户。

---
