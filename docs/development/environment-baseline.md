# 环境基线勘察报告（Phase 0 / Step 1–2）

- **采集时间**：2026-11-16 19:55–20:05 (Asia/Taipei)
- **采集方式**：在实际运行环境中直接执行命令，输出原文摘录，未使用推测数据
- **用途**：Devchievements 规范 §48.2「检查开发环境与工具链」的交付物；同时也是 §29 平台兼容性矩阵的初始证据
- **重要声明**：本文件只记录**实测结果**。没有实测到的一切能力，在 `STATUS.md` 与兼容性矩阵中一律标记为 `Platform Unverified`，不得写成已验证。

---

## 1. 宿主机

| 项 | 实测值 |
| --- | --- |
| 操作系统 | Windows NT 10.0.26300.0（Windows 11 内核，build 26300） |
| 架构 | x64 |
| 用户 | `WINBOOKPRO\\csa` |
| 管理员权限 | **否**（`IsAdmin = False`）→ 无法做系统级安装（MSVC / Windows SDK / winget 源） |
| C: 可用空间 | 340.8 GB / 952.5 GB |
| Shell | `pwsh -Command`（每次调用新进程） |

## 2. 现有工具链

| 工具 | 版本 / 状态 | 备注 |
| --- | --- | --- |
| Git | **2.54.0.windows.1** | 来自 `C:\\Users\\csa\\AppData\\Local\\hermes\\git`；x86_64；default hash sha1 |
| Node.js | **v22.23.2** | `C:\\Program Files\\nodejs\\node.exe` |
| npm | 10.9.8 | 可联网（见 §4） |
| pnpm | **11.28.5**（DSH 内置副本） | `<dsh-runtimes>\\dependencies\\pnpm\\bin\\pnpm.mjs`，需用内置 node 调用 |
| Python | 3.12.14（DSH 内置） | 系统 `python.exe` 只是 WindowsApps 占位符，实际不可用 |
| uv | 0.12.0 | `%LOCALAPPDATA%\\hermes\\bin\\uv.exe` |
| Rust / cargo / rustup | **初始不存在** | Phase 0 中已按用户授权就地安装，见 §6 |
| MSVC / Windows SDK | **不存在** | 无 `vswhere.exe`、无 `Windows Kits`、无 `cl.exe` |
| .NET | 仅有 runtime（`host`/`shared`），**无 SDK** | 不能用于构建 |
| winget | 存在但**不可用** | `winget --version` 返回 `0x8A150021`，`source list` 空 |
| Chrome | 157.0.8103.52 | 可做扩展与 Native Messaging 实测 |
| Microsoft Edge | 157.0.4341.60 | 同上 |
| WebView2 Runtime | 157.0.4341.46（已注册） | Tauri 运行期依赖满足 |
| Brave | **未安装** | 扩展兼容性标记 `Platform Unverified` |

## 3. 网络与代理

| 目标 | 实测 |
| --- | --- |
| DNS 解析 | 正常（github.com → 20.6.7.17） |
| 裸 TCP 443 | 可连通（`Test-NetConnection` True） |
| `curl.exe` 直连 HTTPS | **全部失败**（`schannel: AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS`） |
| 系统 WinINET 代理 | `ProxyEnable=0`，残留 `192.168.3.8:1082`（该地址当前不可连通） |
| npm registry | **可用**（`npm ping` / `npm install` 成功，实测 1.2 s 内装完 `is-number`） |
| Node.js `https` 模块 | **可用**（registry.npmjs.org 返回 200） |
| `static.rust-lang.org` | 通过 Node.js 下载成功（12.8 MB `rustup-init.exe`） |
| `git` 的 schannel 后端 | **失败**（同上 SEC_E_NO_CREDENTIALS） |
| `git -c http.sslBackend=openssl ls-remote https://github.com/...` | **成功** |

**结论（本机专属，需写入开发文档）**：本机上「直连 HTTPS 的命令行工具」普遍因 schannel 凭据问题失败，但 **Node.js 的 OpenSSL 栈与 npm 正常**，并且 **git 切换 `http.sslBackend=openssl` 后正常**。

因此：
- 所有需要访问网络的脚本一律用 Node.js 实现（Python / curl / Invoke-WebRequest 在本机不可靠）。
- Git 远程操作建议在本机加 `git config --global http.sslBackend openssl`（**未自动执行**，属于修改用户全局配置，需用户确认）。
- 这是**宿主环境缺陷**，不是产品缺陷；不得把该 workaround 写进产品代码路径。

## 4. 沙箱与文件权限

- DSH 文件策略：`workspace-write`，可写范围 = `C:\\Users\\csa\\Documents\\deepseek-harness\\default-workspace` 及平台临时目录。
- 实测：工作区写入 OK；`%TEMP%` 写入 OK。
- **实测拒绝**：`npm` 默认缓存目录 `%LOCALAPPDATA%\\npm-cache` 写入被拒（`EPERM`）→ 所有 npm 操作必须显式 `--cache <工作区内路径>`。
- `fsutil volume diskfree` 与 `Get-Volume` 被拒（access denied）→ 用 `.NET DriveInfo` 获取磁盘信息。
- 宿主自述：「File sandbox enforcement is partial on this host」。

## 5. 对 Phase 0 验收项的直接影响

| 规范要求（§30.5） | 本机状态 | 处理 |
| --- | --- | --- |
| 工具链正常 | 部分达成 | Node/pnpm/Git 就绪；Rust 见 §6；无 MSVC |
| 项目可构建（Cargo + pnpm） | 可达成 | Rust 侧用 `x86_64-pc-windows-gnu`，前端用 pnpm |
| Tauri 可运行 | **受阻** | GNU 目标下 Tauri 2 Windows 打包不可靠；MSVC 缺失且无管理员权限 → 标 `Platform Unverified`，需用户升权安装 VS Build Tools 后补测 |
| 扩展可构建 | 可达成 | WXT + MV3，纯 JS 工具链 |
| SQLite + SQLx 可读写 | 可达成 | `libsqlite3-sys` bundled 特性，GNU 目标可编译 |
| Native Messaging 最小验证 | 可达成（有条件） | 需真实 Chrome/Edge 安装扩展；手动加载解包扩展可做 |
| WASM 编译与加载验证 | 编译可达成；**MV3 内加载需真实浏览器实测** | Chrome/Edge 157 |
| Git 行为实验 | **可完整达成** | Git 2.54.0 可用 |
| 桌面通知能力实测 | 受限 | 无 GUI 会话控制权时需人工确认 |

## 6. Rust 工具链（用户已授权就地安装）

- 决策：**不安装到系统目录**，`RUSTUP_HOME` / `CARGO_HOME` 均落在工程工作区内，无需管理员权限，可整体删除回滚。
- 目标三元组：`x86_64-pc-windows-gnu`（原因：本机无 MSVC，GNU 目标自带链接所需组件）。
- 附加目标：`wasm32-unknown-unknown`（WASM spike 用）。
- 已知局限：**Tauri 的 Windows 打包在 GNU 目标下不受官方支持**，因此 Phase 0 的「Tauri 可运行」验收会在本机标记为 `Platform Unverified`，需要 MSVC 或另一台机器补测。该局限会写进 ADR。

### 实测结果（已完成）

| 项 | 结果 |
| --- | --- |
| rustup-init 版本 | 1.100.0 通道对应的最新 rustup |
| 默认工具链 | `stable-x86_64-pc-windows-gnu` |
| rustc | 1.100.0 (0ef1bc386 2026-11-09)，LLVM 23.1.5 |
| cargo | 1.100.0 (3acc79c0c 2026-11-05) |
| 已装 target | `x86_64-pc-windows-gnu`、`wasm32-unknown-unknown` |
| 安装位置 | `<workspace>/_toolchain/{rustup,cargo}`（工程外，可整体删除） |
| 磁盘占用 | 约 1.6 GB |

**冒烟测试（真实执行，非推测）**：

| 测试 | 命令 | 结果 |
| --- | --- | --- |
| 编译+链接 | `cargo build` | ✅ 成功（0.68 s） |
| 单元测试 | `cargo test` | ✅ 1 passed, 0 failed |
| 运行二进制 | `./target/debug/cargotest.exe` | ✅ 输出 `hello 42` |
| WASM 编译 | `cargo build --target wasm32-unknown-unknown` | ✅ 成功，产物 1,667,910 bytes（含未裁剪 std） |

**结论**：GNU 目标在本机**可以完整完成 编译 → 链接 → 运行 → 测试**，因此 Rust Core / Helper / Golden Tests / WASM spike 全部可真实执行。

**唯一带风险的例外**：Tauri 2 的 Windows 打包在 GNU target 下不受官方支持，本机又无 MSVC，故「Tauri 可运行」在本环境记为 `Platform Unverified`。

### 环境变量约定（所有脚本必须显式设置）

```powershell
$env:RUSTUP_HOME = "<workspace>\_toolchain\rustup"
$env:CARGO_HOME  = "<workspace>\_toolchain\cargo"
$env:PATH        = "<workspace>\_toolchain\cargo\bin;" + $env:PATH
```

因为安装时使用了 `--no-modify-path`，未写入用户环境变量，属于**完全隔离、零副作用**的安装。工程内提供 `scripts/env.ps1` 统一下发这些变量。

## 7. 变化记录

| 时间 | 事件 |
| --- | --- |
| 2026-11-16 19:55 | 首次环境勘察完成 |
| 2026-11-16 19:59 | 与用户确认：项目根目录 = `default-workspace\\devchievements\\`；批准就地安装隔离 Rust |
