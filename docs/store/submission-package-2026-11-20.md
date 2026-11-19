# 商店提交材料包（v0.1.0）— 2026-11-20 00:55 状态

> 本文件回答一个问题：**现在就能提交吗，还差什么。**
> 带 🔴 的只有你能做；其余都已完成并留有证据。

## 已就绪（上传即用）

| 材料 | 位置 / URL | 证据 |
| --- | --- | --- |
| MSIX 包 | `C:\Documents\Devchievements\Devchievements.msix`（9,162,830 B，00:31 构建） | `build-msix.mjs` 11 项全过；`signtool verify /pa` 成功；包内二进制与本地 release SHA256 一致（`F60BDE0E…09B865`）；`--preview-sound common` 在已安装副本上耗时 1188 ms（≥ 838 ms 下限），播放闸门在包内生效 |
| 隐私政策 URL | <https://applepinea.github.io/Devchievements/> | **已实测 200**，页面含 "Privacy Policy" 与联系邮箱（此前 Pages 源指向 branch/README，渲染的是旧 README；已切到 GitHub Actions 源并由 pages.yml 部署成功） |
| 商店文案 | [store-submission-checklist.md](store-submission-checklist.md) §4 | 名称、类别、短描述（中英）、详细描述、7 个关键词 |
| 认证备注 | [store-certification-notes.md](store-certification-notes.md) §1 | 英文可直接粘贴；WebView2 依赖行为、退出码 2、helper 复制、数据目录都写明了 |
| 截图 4 张 | `docs/store/screenshots/01–04*.png`（1280×800） | 仪表盘 / 成就 / 活动 / 设置 |
| CI | GitHub Actions 三个 workflow **全部首次跑绿**（run 44568079398，含多 Git 版本矩阵） | 0.7 状态可从 `Platform Unverified` 升级 |

## 本轮修掉的阻塞

| 问题 | 根因 | 修复 |
| --- | --- | --- |
| CI 三个 Git 矩阵 job 全挂 | 资产名写成 `PortableGit-<ver>-64bit.7z.exe`，GitHub 404 后返回 HTML 页，被当成自解压包保存 | 改为 `64-bit`（对照 GitHub release API 核实），并加 10 MB 大小守卫（commit c3c2b30） |
| 隐私政策 URL 打开是旧 README | Pages 源是 "deploy from branch / root"，Jekyll 渲染了 README 而不是 pages.yml 的产物 | 已切到 GitHub Actions 源；pages.yml 手动触发部署成功并实测页面内容 |

## 🔴 只剩你能做的

1. **注册 Partner Center** 并创建应用名 `Devchievements`（个人账号免费，需身份证 + 自拍验证）。
2. **上传** `Devchievements.msix` → Packages；填 Properties / Age ratings（全选无）/ Store listings（§4 文案 + 4 张截图）/ 定价免费 / Notes for certification（§1 备注）。
3. **提交前自查**（store-certification-notes.md §3）：干净 Win10 虚拟机三项 —— 缺 WebView2 弹原生框、点是后继续启动、断网回落下载页退出码 2。这三项只有真机能测。

## 已知不会挡审核但要知道的

- MSIX 无时间戳签名（`File is not timestamped`）——商店会重签，不影响提交。
- 包是测试证书签的——商店重签，同理。
- 截图是 11-18 的界面，此后 Builder 页有改动；如果想展示 Builder，需要重截，但不是必需项。
