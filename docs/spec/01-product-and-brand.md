# 第 1 章　产品定位与品牌

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 1. 产品定位与品牌

## 1.1 产品定位

Devchievements 是面向开发者的游戏化成就伴侣。它追踪本地 Git 仓库及受支持 GitHub 工作流中的活动，并转换为：

- 永久成就、每日等级与每日里程碑。
- 连续活跃记录与仓库活动统计。
- 游戏化通知、自定义提示音、成就图标和弹窗。
- 用户自定义成就与主题。

核心体验：**每一次 Commit 都不只是一次代码提交，也可以成为一次值得庆祝的进度。**

## 1.2 品牌规范

- 正式名称：Devchievements；副标题：`Achievements for Developers`。
- 内部代号 GSA 仅用于开发、目录和内部讨论，不用于面向用户的品牌展示。
- 产品名称、仓库名称、桌面应用名称、浏览器扩展名称保持一致。

## 1.3 与 GitHub 官方功能的区别

GitHub 本身有官方的个人主页 Achievements 徽章功能。Devchievements 必须明确区分：

- GitHub 官方 Achievements 是 GitHub 平台内的个人主页徽章。
- Devchievements 是独立第三方软件，提供本地 Git 活动追踪、成就判定、提示音、桌面弹窗和自定义游戏化体验。

不得暗示 Devchievements 由 GitHub 官方开发、认可或赞助。

## 1.4 商标与视觉规范

- 不得未经授权使用 GitHub 或 Steam 的官方 Logo、商标资产或容易造成官方关联误解的视觉元素。
- 可以采用游戏成就系统的设计语言，但不得包装成官方 Steam 成就系统。
- 商店标题、副标题、关键词、图标中不得出现 Steam；描述中使用 `game-style achievements` 等通用表达。内部设计文档可以用 Steam 风格作为视觉参考。
- 正式发布前检查：产品名称商标、域名、GitHub 仓库名、Microsoft Store、Chrome Web Store、Microsoft Edge Add-ons、Bundle Identifier。
- Bundle ID 示例 `com.yourdomain.devchievements`，`yourdomain` 只是占位符，必须由用户确认真实域名。
- 此前的快速同名搜索不能代替正式商标检索。

---
