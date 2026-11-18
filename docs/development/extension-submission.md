# Edge 扩展提交材料

> 目标：把 v0.1.0 提交到 Microsoft Edge 加载项商店（Partner Center → Microsoft Edge program）。

## 0. 提交物

| 文件 | 位置 |
| --- | --- |
| 扩展包 | `C:\Documents\Devchievements\Devchievements-extension-0.1.0.zip` |
| 重新生成 | `cd apps/extension && node ../../node_modules/wxt/bin/wxt.mjs build`，再把 `.output/chrome-mv3/*` 压成 zip（内容在根目录，**不能**套一层文件夹） |

版本号来自 `apps/extension/package.json`，**不要**再在 `wxt.config.ts` 里写死。
之前两边都写 `0.0.0`，商店会直接拒绝这个版本号。

## 1. 单用途说明（Single purpose）

> Devchievements records the developer's own GitHub activity (pull requests, releases and
> issues they can already see) and forwards it to the Devchievements desktop app on the
> same computer, so that activity can be turned into achievements. It does nothing else.

## 2. 权限理由（Permission justification）

这一节是审核重点，逐条填写：

| 权限 | 理由（英文，直接粘贴） |
| --- | --- |
| `nativeMessaging` | Used to send activity summaries to the Devchievements desktop application installed on the same computer. This is the only channel the extension uses; nothing is sent to any server. |
| `storage` | Stores the user's settings and a bounded queue of pending events while the desktop application is not running. |
| `notifications` | Shows a single reminder when the desktop application is not installed, and nothing while it is. |
| `https://github.com/*` | Reads the GitHub pages the user is already viewing to identify pull requests, releases and issues belonging to the user's own account. No other site is requested. |

## 3. 远程代码（Remote code）

**否。** MV3 不允许远程代码，本扩展也不使用：所有脚本随包分发。
内联进包内的 WebAssembly（`wasm/devchievements_core.wasm`）是**随包发布**的本地文件，
不是远程加载；CSP 中的 `'wasm-unsafe-eval'` 只用于实例化这个本地模块。

## 4. 隐私声明要点

与 [docs/privacy.md](../privacy.md) 保持一致：

| 问题 | 回答 |
| --- | --- |
| 收集个人数据？ | 否。数据只留在本机 |
| 上传到服务器？ | 否 |
| 卖给第三方？ | 否 |
| 是否需要桌面版？ | 完整功能需要；单独安装时扩展会明确说明 |

隐私政策 URL：`https://applepinea.github.io/Devchievements/`（Pages 开启后生效）

## 5. 商店信息

| 字段 | 内容 |
| --- | --- |
| 名称 | Devchievements |
| 简短描述 | Achievements for Developers |
| 类别 | Developer tools |
| 语言 | English, 简体中文 |

### 详细描述（英文）

> Turn your GitHub activity into achievements.
>
> Devchievements observes the pull requests, releases and issues you already see on
> GitHub and forwards a summary to the Devchievements desktop application on the same
> computer, where it becomes permanent achievements, daily ranks and streaks.
>
> - Nothing leaves your computer. No account, no server, no telemetry.
> - Only github.com pages you are already viewing are read.
> - Works with the Devchievements desktop app; without it the extension tells you so
>   rather than pretending to do something.
>
> Devchievements is an independent third-party application and is not affiliated with,
> endorsed by, or sponsored by GitHub.

### 详细描述（简体中文）

> 把你已经在 GitHub 上看到的活动变成成就。
>
> Devchievements 读取你正在浏览的 Pull Request、Release 与 Issue 页面，把摘要交给同一台电脑上的
> Devchievements 桌面版，转成永久成就、每日等级和连续记录。
>
> - 数据不离开你的电脑：没有账号、没有服务器、没有遥测。
> - 只读取你正在浏览的 github.com 页面。
> - 需要配合桌面版使用；没有桌面版时会明确提示，而不是假装在工作。
>
> Devchievements 是独立第三方软件，与 GitHub 无隶属、认可或赞助关系。

### 截图

1. 扩展弹窗（当前状态与连接情况）
2. GitHub 页面上被识别到的活动

## 6. 提交前自查

- [ ] zip 内容在根目录（`manifest.json` 在最外层）——已核对
- [ ] 版本号非 `0.0.0`——已修为 `0.1.0`
- [ ] `manifest.json` 引用的文件都在包内——已核对（`background.js`、`popup.html`、`content-scripts/github.js`）
- [ ] 认证备注写清「需要桌面版」
- [ ] 隐私政策 URL 可访问

## 7. 已知局限

| 项 | 说明 |
| --- | --- |
| 需要桌面版才有意义 | 单独提交扩展时审核会问；备注里已说明 |
| 只有 Homebrew 路径的图标 | 商店需要 128×128 图标，当前由 `wxt` 默认生成，**上架前应换成正式图标** |
- [ ] 换成正式图标后再提交
