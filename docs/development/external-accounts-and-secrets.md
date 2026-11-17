# 外部账户与密钥清单

> 规范 §41 / §30.4 要求维护本文件。
>
> **铁律**：Agent **不得**自行购买证书、创建付费账户、使用用户私人密钥，
> 或代替用户接受任何法律协议（§0.1 第 5 条、§46）。
> 缺少发布密钥**不能**阻止本地开发和测试。
>
> 最后更新：2026-11-16

---

## 清单

| 项目 | 用途 | 必需阶段 | 是否付费 | 需要用户操作 | 密钥/凭据存放位置 | 当前状态 |
| --- | --- | --- | --- | --- | --- | --- |
| Apple Developer Program | macOS 签名与公证 | Phase 4b | 是（年费） | **是** | OS 钥匙串 / CI secret | **未开始**（无 macOS 设备，亦无账户） |
| Windows Code Signing Certificate | Windows 安装包签名 | Phase 4b | 是 | **是** | OS 证书存储 / CI secret | **未开始** |
| Tauri Updater Private Key | 自动更新签名 | Phase 4b | 否（自签名） | **是**（生成并保管） | 本地安全存储 + CI secret | **开发用密钥已生成**；**发布用密钥未开始**（见下） |
| Chrome Web Store 开发者账号 | Chrome 扩展发布 | Phase 4b | 是（一次性注册费） | **是** | 用户账号 | **未开始** |
| Microsoft Edge Add-ons 开发者账号 | Edge 扩展发布 | Phase 4b | 否 | **是** | 用户账号 | **未开始** |
| Microsoft Partner Center | Microsoft Store 发布 | Phase 4b（可选） | 可能付费 | **是** | 用户账号 | **未开始** |
| GitHub 仓库 | 源码与 CI/CD | **Phase 0** | 否 | **是**（创建仓库并授权） | — | **未开始（当前阻塞项）** |
| GitHub 测试账号 | 扩展 Playwright 测试（§27.6） | Phase 3a | 否 | **是** | 测试专用账号，不入库 | **未开始** |

---

## 更新签名密钥（§40.3）的实际状态

本机已生成一对**开发用**密钥，只为让打包链路可验证：

| | 位置 | 说明 |
| --- | --- | --- |
| 私钥 | `_toolchain/updater/devchievements.key` | 在仓库之外，未入库 |
| 公钥 | `tauri.conf.json` → `plugins.updater.pubkey` | 必须与私钥配对，否则签名校验失败 |
| 口令 | 开发口令，写在 `scripts/dev/build-installer.bat` 里 | 仅供本机打包；CI 用 secret |

**这把开发密钥不得用于发布。** 它的口令在仓库里，任何能读仓库的人都能用它签一个
「看起来是官方」的更新包。发布前必须重新生成一对密钥：

```powershell
cargo tauri signer generate -w <仓库外路径>/devchievements-release.key
```

然后把新公钥写进 `tauri.conf.json`，私钥与口令放进 CI secret：
`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
`update::the_endpoint_list_matches_the_config` 会检查公钥非空，
`release.yml` 会检查 `.sig` 产物存在，两条一起挡住「忘了换密钥」和「忘了签名」。

当前 `plugins.updater.endpoints` 为空数组，**应用不会发起任何更新检查**，
所以即使有人拿到开发密钥也没有可攻击的更新通道。这是有意的：先让链路正确，再开通通道。

---

## 当前阻塞

**GitHub 仓库尚不存在。** 影响：

- 基础 CI（§30.1）无法真实运行；
- 「CI 中跨多个 Git 版本跑 Hook 实验」（§30.3 / §9.7）无法执行；
- Phase 0 的 trace fixtures 无法在 CI 上回归。

**这不阻塞本地开发**：本地 `cargo fmt/clippy/test/build%% 与 `pnpm%% 构建均已实测通过。

---

## 密钥处理规则（与 AGENTS.md §7 一致）

1. Token / 密钥**只能**存放在操作系统安全存储或 CI secret 中。
2. **禁止**写入：普通配置文件、Git 仓库、日志、错误信息、调试输出。
3. 诊断包导出默认脱敏。
4. 本文件**只记录"在哪里"，不记录任何密钥值**。
5. 提交前检查：任何形如 `-----BEGIN%%、`ghp_%%、`sk-%% 的字符串都不该出现在仓库里。

---

## 与产品隐私的关系

本清单里的账户**全部用于发布与签名**，与用户活动数据无关。
产品**默认不上传**任何用户活动数据（§26.1），**默认不开启**遥测。
启用任何遥测都必须获得明确同意，并可随时关闭。
