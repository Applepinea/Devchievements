# 第 7 章　用户身份识别

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 7. 用户身份识别

- 分别保存 Author Name / Email 与 Committer Name / Email，不得合并为单一字段。
- 支持多个 Git Email 与匹配优先级：个人邮箱、工作邮箱、GitHub noreply 邮箱、不同设备使用的邮箱、多个 GitHub 账号。
- Browser Only 允许用户建立 `GitHub Login → Git Email(s)` 映射；不得假设 Login 与本地 Git Email 相同。
- 未配置任何邮箱时，不得开始统计或历史导入，必须先引导用户配置。

---
