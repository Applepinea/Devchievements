# 第 13 章　永久成就系统

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 13. 永久成就系统

## 13.1 成就性质

永久成就是一次性解锁的长期成就，与每日等级和每日里程碑完全分离，必须分开设计数据模型和存储（`unlocked_achievements` 与 `daily_milestones`）。永久成就只解锁一次，已解锁成就不会因后续统计下降而撤销。

## 13.2 首发成就类别

- Commit：First Commit、10 / 100 / 1000 Commits。
- Release：First Release 等。
- PR、Issue、Repository Milestones。
- Streak Milestones（基于连续活跃投影）。
- Hidden Achievements。

每个成就必须声明 `required_capabilities`（见「数据来源能力」）。Desktop Only 下依赖浏览器事件的成就显示为「需要浏览器扩展」，不得显示成可达成但永远为零的进度。

## 13.3 版本化规则

每个内置成就必须包含：`achievement_id`、`achievement_version`、`rule_version`、`introduced_at`、`retired_at`、`statistics_profile`、`eligibility_policy`、`retroactive_policy`、`required_capabilities`。

- 内置永久成就使用固定、版本化的官方统计口径，不得因用户修改每日等级阈值或普通过滤器而改变。
- 例：内置 Legendary Day 始终要求 20 次有效 Commit，即使用户把每日 Legendary 阈值改为 15。
- 用户自定义成就可以使用用户指定的统计范围。
- 规则升级不得撤销已解锁的成就。

**回填策略（`retroactive_policy`）取值**

- `never`：只对规则生效后新到达的事件生效。
- `from_install`：只对首次启动之后产生的事件生效。
- `full_history`：对事件库中的全部事件生效（包括用户导入的历史）。
- `user_choice`：创建或升级规则时询问用户。
- 内置成就默认 `full_history`，作用范围是事件库中已有的事件：默认事件库从安装时开始，用户导入历史后自动覆盖导入的数据。自定义成就默认 `user_choice`。历史回填一律静默。

## 13.4 解锁记录与历史回填

- 解锁记录必须包含：解锁时间、解锁来源（`realtime` / `delayed` / `backfill` / `import`）、使用的规则版本、是否已查看（`viewed_at`）。
- 历史导入与回填默认静默，不得伪装成实时解锁。

## 13.5 Hidden Achievements

支持隐藏名称与描述、解锁后揭示、独立通知偏好。

---
