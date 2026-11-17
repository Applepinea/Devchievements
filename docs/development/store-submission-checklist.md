# 微软商店提交材料清单
> 目标：把 v0.1.0 作为**首版**提交到 Microsoft Store（MSIX 路线）。
> 用法：按顺序逐项完成。带 🔴 的必须由你完成（我无法代替）；带 🟢 的我已完成或可以代做。

---

## 0. 前置状态（已就绪）

| 项 | 状态 | 证据 |
| --- | --- | --- |
| MSIX 包可构建、可安装、可运行 | 🟢 已完成 | `scripts/msix/build-msix.mjs`，10 项探测全过 |
| 版本号 `0.1.0`（四段 `0.1.0.0`） | 🟢 已完成 | `tauri.conf.json` |
| 缺 WebView2 不白屏、可自愈 | 🟢 已完成 | `installer-hooks.nsh` + 原生双语框 |
| 认证备注（英文） | 🟢 已写好 | [store-certification-notes.md](store-certification-notes.md) |
| 卸载不删用户数据 | 🟢 已完成 | 数据在 `%LOCALAPPDATA%\devchievements` |
| 自定义成就真的会解锁 | 🟢 已修复 | `scripts/e2e/user-achievements.mjs` |

---

## 1. 🔴 注册 Partner Center（一次性）

1. 打开 <https://partner.microsoft.com> → 注册 **Microsoft Store 应用**开发者账号（与 Edge 扩展同一账号即可）
2. 个人账号免注册费，需要：政府签发身份证件 + 自拍
3. 审核通过后创建应用名 **Devchievements**

⏱ 约 30 分钟 + 等待验证。

---

## 2. 🟢 图标（已完成）

**源图**：`apps/desktop/src-tauri/icons/source-icon.webp`（1024×1024，用户提供）。
`icon.ico` 已重新生成为 16/24/32/48/64/128/256 多尺寸，MSIX 载荷已用新图标重打并复测。


| 文件 | 尺寸 | 用途 |
| --- | --- | --- |
| `StoreLogo.png` | 50×50 | 商店列表 |
| `Square44x44Logo.png` | 44×44 | 开始菜单/任务栏 |
| `Square150x150Logo.png` | 150×150 | 磁贴 |
| `Square310x310Logo.png` | 310×310 | 大磁贴 |
| `Wide310x150Logo.png` | 310×150 | 宽磁贴 |

生成后我重打包并跑探测。

---

## 3. 🟡 隐私政策（正文与发布流程已完成，等你开启 Pages）

商店要求一个**可公开访问的 URL**。内部文档 `docs/spec/26-…` 不算。

**最省事的方式**：在仓库里放 `docs/privacy.md`，开 GitHub Pages，得到
`https://applepinea.github.io/Devchievements/privacy`。

🟢 隐私政策**正文我可以写**，内容基于已经实现的真实行为：

| 必须写清的点 | 我们的事实 |
| --- | --- |
| 是否收集数据 | **不收集**，数据只存本机 `%LOCALAPPDATA%\devchievements` |
| 是否联网 | 默认**不联网**；仅「检查更新」时访问更新源 |
| 遥测 | **默认关闭**，开启需显式同意 |
| 是否上传 Git 历史/提交信息/仓库名 | **否** |
| 隐私模式 | 隐藏仓库名、提交信息、PR 标题，诊断导出默认脱敏 |
| 卸载后数据 | **保留**，需手动删除 |
| 联系方式 | 需要一个你能公开的邮箱 |

🟢 正文：已完成 [docs/privacy.md](../privacy.md)，联系邮箱 `csah1350@gmail.com`。
🟢 发布：已完成 `.github/workflows/pages.yml`（推送即渲染发布）。

**你需要做一步**：GitHub 仓库 → Settings → Pages → Source 选 **GitHub Actions**。
完成后 URL 为 `https://applepinea.github.io/Devchievements/`。

---

## 4. 商店文案

### 4.1 基本信息

| 字段 | 内容 |
| --- | --- |
| 产品名 | `Devchievements` |
| 类别 | Developer tools / 开发者工具 |
| 价格 | 免费 |
| 支持语言 | English、简体中文 |

### 4.2 简短描述（≤ 200 字符）

> 把本地 Git 活动变成永久成就、每日等级和连续记录，用游戏风格弹窗和提示音庆祝。所有数据只留在你的电脑上。

英文：

> Turn your local Git activity into permanent achievements, daily ranks and streaks, celebrated with game-style notifications. Everything stays on your PC.

### 4.3 详细描述

> **用成就记录你的开发历程**
>
> Devchievements 悄悄观察你本机的 Git 提交，把每一次提交变成看得见的进步：
>
> - **永久成就** —— 第一次提交、连续活跃、里程碑，一旦解锁永不丢失
> - **每日等级** —— 每天的提交量对应一个等级，当天升级会有专属提示
> - **游戏风格通知** —— 弹窗 + 提示音，可自定义位置、尺寸、动画和音效
> - **成就画廊** —— 按稀有度浏览你解锁的记录
> - **掌握在自己手里** —— 隐私模式可隐藏仓库名与提交信息；数据只存在本机，不上传
>
> **完全本地优先**
> 不需要注册账号。除了「检查更新」，程序不会主动联网；提交历史、仓库名、提交信息都留在你的电脑上。
>
> **浏览器扩展（可选）**
> 安装扩展后，可识别你在 GitHub 上的 PR、Release 等活动。不安装也可以正常使用桌面版。
>
> *Devchievements 是独立第三方软件，与 GitHub 无隶属、认可或赞助关系。*

### 4.4 搜索关键词（最多 7 个）

> `git`、`achievements`、`developer`、`productivity`、`git 成就`、`开发者工具`、`streak`

---

## 5. 截图（至少 1 张，建议 4 张）

商店要求 1280×800 或 640×480。**必须来自真实界面**。

| 序号 | 画面 | 说明重点 |
| --- | --- | --- |
| 1 | 主仪表盘 | 提交数、连续天数、成就数 |
| 2 | 成就画廊 | 已解锁的稀有度分布 |
| 3 | 一次真实弹窗 | 游戏风格通知 + 音效图标 |
| 4 | 设置页 | 语言、隐私模式、弹窗自定义 |

🟢 我可以启动应用、用真实数据摆好界面并截图（需要你本机数据已有一部分提交记录）。

---

## 6. 年龄分级与合规

| 项 | 建议 |
| --- | --- |
| 年龄分级问卷 | 全部选「无」：无暴力、无性内容、无赌博、无用户生成内容、**无社交功能** |
| 应用能力声明 | 只用 `runFullTrust` |
| 是否收集儿童数据 | 否 |
| 是否需要网络 | 否（首次缺 WebView2 时自动安装需要网络，见认证备注） |

---

## 7. 🔴 上传与提交

1. Partner Center → 应用 → **Packages** → 上传 `Devchievements.msix`
2. **Properties** → 类别、系统要求（Windows 10 2004+ / x64）
3. **Age ratings** → 完成问卷
4. **Store listings** → 填入第 4 节文案 + 第 5 节截图
5. **Pricing and availability** → 免费、全部市场
6. **Submission options** → **Notes for certification** 粘贴 [认证备注](store-certification-notes.md) 第 1 节
7. 提交审核

⏱ 审核通常 1–5 个工作日。

---

## 8. 提交前自查（逐项打勾）

- [ ] 干净 Windows 10 虚拟机（**未跑过 Windows Update**）装包后启动，弹出**原生双语框**而不是白窗
- [ ] 在同一台点「是」，运行时装好后**程序继续启动**，不需要重启
- [ ] 断网点「是」，回落到打开下载页，退出码为 **2**
- [ ] 装好后在 git 仓库 commit，托盘出现、弹窗出现、成就解锁
- [ ] 卸载后 `%LOCALAPPDATA%\devchievements` **仍然存在**
- [ ] 商店文案里没有出现「Steam」字样，没有使用 GitHub 官方 Logo

---

## 9. 需要你提供的东西（汇总）

| # | 需要 | 状态 |
| --- | --- | --- |
| 1 | 图标源图 | 🟢 已提供并生成完毕 |
| 2 | 公开联系邮箱 | 🟢 `csah1350@gmail.com` 已写入政策 |
| 3 | 隐私政策 | 🟡 已写好，需在仓库 Settings → Pages 选 GitHub Actions |
| 4 | 干净 Win10 虚拟机三项验证 | 🔴 只有你能做 |
| 5 | Partner Center 账号与提交 | 🔴 只有你能做 |

其余（文案、截图、打包、认证备注）我可以全部完成。
