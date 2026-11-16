# 第 5 章　事件系统

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 5. 事件系统

## 5.1 支持的事件类型

首发：Commit Created、Release Published、Pull Request Opened、Pull Request Merged、Issue Opened、Issue Closed、Repository Created、Star Given、Local Merge。

P2：Star Received（需要用户授权 GitHub API 或轮询，不能假设扩展总能观察到）。

## 5.2 统计投影

连续活跃天数、每日等级、累计数量等是统计投影，不是原始事件，不得建模成由 Hook 或浏览器直接采集的事件。

## 5.3 标准事件结构

每个标准事件至少包含：

```text
event_id, event_type, source, source_confidence,
canonical_repository_id, external_id, commit_sha,
occurred_at, observed_at, received_at,
local_date, timezone, date_policy,
payload, schema_version
```

- `occurred_at`：事件实际发生时间；`observed_at`：检测到事件的时间；`received_at`：系统接收时间。
- `local_date`：按当时日期策略计算的本地日期；`timezone`：事件对应的时区。
- 必须明确各事件类型的必需字段；`event_id` 同时作为幂等 ID。

## 5.4 来源可信度

支持 `verified_local`、`browser_confirmed`、`imported`、`unverified`。

- Git Commit 的作者信息可以被伪造，邮箱匹配仅用于归属判断，不是身份认证。
- Local Git 事件由 Companion 复核：仓库、Commit SHA、作者邮箱、parent 信息。
- 浏览器事件必须尽可能验证：扩展来源、当前 GitHub 登录账号、Repository ID、Commit SHA / Release ID / Tag、操作是否真正成功。
- History Import 标记为 `imported`；`unverified` 默认不计入正式成就统计。

---
