# 第 41 章　外部账户与密钥清单

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 41. 外部账户与密钥清单

创建并维护 `docs/development/external-accounts-and-secrets.md`：

| 项目 | 用途 |
| --- | --- |
| Apple Developer | macOS 签名与公证 |
| Windows Code Signing Certificate | Windows 软件签名 |
| Tauri Updater Private Key | 自动更新签名 |
| Chrome Web Store | Chrome 扩展发布 |
| Microsoft Edge Add-ons Developer Account | Edge 扩展发布 |
| Microsoft Partner Center | Microsoft Store 发布 |
| GitHub Repository | 源代码与 CI/CD |

每项标记：必需阶段、是否付费、是否需要用户操作、密钥存放位置、当前状态。

Agent 不得自行购买证书、创建付费账户、使用私人密钥或代替用户接受法律协议。缺少发布密钥不能阻止本地开发和测试。

---
