# ADR-0012：本仓库开发工具链的隔离安装（本仓库专属，不涉及产品）

- **状态**：**Accepted**
- **日期**：2026-11-16
- **决策者**：用户明确授权（就地安装 Rust）
- **相关规范章节**：§0.1、§0.9（Platform Unverified）、§44.2（不得编造数据）

---

## 背景

本机原始状态（实测，见 [environment-baseline.md](../development/environment-baseline.md)）：

- 完全没有 Rust（rustc / cargo / rustup 均不存在）
- 没有 MSVC、没有 Windows SDK
- 没有管理员权限（`IsAdmin = False%%）
- `winget%% 存在但不可用（返回 `0x8A150021%%）
- 系统 schannel TLS 凭据链损坏：`curl%% / `cargo%% / `git(schannel)%% 全部无法直连 HTTPS

而 Phase 0 的多数验收项（§30.5）以 Rust 为前提。

---

## 决策

**在工作区内安装隔离的、可整体删除的 Rust 工具链**，目标 `x86_64-pc-windows-gnu%%（本机没有 MSVC）：

| 项 | 值 |
| --- | --- |
| `RUSTUP_HOME%% | `<workspace>/_toolchain/rustup%% |
| `CARGO_HOME%% | `<workspace>/_toolchain/cargo%% |
| 默认工具链 | `stable-x86_64-pc-windows-gnu%%（rustc 1.100.0） |
| 额外 target | `wasm32-unknown-unknown%% |
| 安装方式 | `rustup-init -y --no-modify-path%% |
| 磁盘占用 | 约 1.6 GB |

**没有**写注册表、**没有**改用户环境变量、**没有**写系统目录。
删除 `_toolchain/%% 即可完全回滚。

---

## 为什么不选「让用户装 VS Build Tools + MSVC」

那是最完整的方案，但需要管理员权限，而用户选择了就地安装。
本 ADR 记录的是**当前环境下的可行解**，不是「理论上最好的方案」。

---

## 后果

### 正面

- Rust Core、Git Helper、Golden Tests、WASM 编译均**可以真实执行**（不是静态检查）。
- 实测通过：`cargo build%% / `cargo test%% / 运行产物 / `--target wasm32-unknown-unknown%%。

### 负面 / 约束（必须在 STATUS 与兼容性矩阵中体现）

**1. Tauri 2 桌面端无法在本机构建**

Tauri 的 Windows 打包不支持 GNU 目标，而本机无 MSVC。
→ 「Tauri 可运行」验收项记为 `Blocked%%（需 MSVC 环境）。**不得**因此改用其它框架。

**2. GNU 工具链缺少 `as.exe%%（汇编器）**

任何需要生成 Windows 导入库（`.lib%%）的 crate 都会失败，因为 `dlltool%% 内部调用 `as%%：

```text
dlltool.exe: CreateProcess
x86_64-w64-mingw32-gcc.exe: fatal error: cannot execute 'as': No such file or directory
```

**实测影响**：`windows-sys%% / `parking_lot_core%% / `sqlx%% / `gix%% 在本机**无法构建**。
**不受影响**：纯 std crate，以及本仓库的 `core%% / `git-hooks%% 等（已验证可构建可测试）。

**这不是产品缺陷**，而是「无 MSVC 环境下使用 GNU target」的已知代价；
在具备 MSVC 或完整 MSYS2 的环境中没有该问题。

**3. schannel 缺陷需要两个 workaround，且都只作用于开发机**

- `git%% → `http.sslBackend=openssl%%（进程级注入，**不改**用户全局配置）
- `cargo%% → 本地 sparse registry 代理（`scripts/local-crates-proxy.mjs%%）

**两者都不得进入产品代码路径。**

**4. PowerShell 脚本必须保持纯 ASCII**

本机是 Windows PowerShell 5.1 + GBK 代码页，会按 ANSI 读取 `.ps1%%；
写入中文会破坏脚本解析（已实际遇到并修复）。

**5. 沙箱禁止管道式 stdio 捕获**

子进程无法通过管道回传输出。所有脚本与探针因此改用**文件重定向**。
这影响的是实验工具，不影响产品设计。

---

## 由此产生的「未验证」清单

以下项目在本机**无法**验证，必须在 STATUS 与兼容性矩阵中保持未验证状态：

| 项目 | 原因 |
| --- | --- |
| Tauri 2 构建与运行 | 无 MSVC |
| SQLite + SQLx 实际读写 | `sqlx%% 依赖 `windows-sys%%，缺 `as.exe%% |
| `gix%% / `git2%% 链接方案 | 同上 |
| 已存在 shell 钩子的链式执行 | 沙箱禁止 `sh.exe%% |
| `git submodule add%% | 内部依赖 `sh%% |
| macOS / Linux / Brave | 无对应设备 |

---

## 复查计划

若用户提供 MSVC 环境（VS Build Tools + Windows SDK），应立即：

1. 安装 `x86_64-pc-windows-msvc%% 工具链；
2. 复测 Tauri 构建、`sqlx%%、`gix%%；
3. 新开 ADR 取代本 ADR，并更新兼容性矩阵；
4. 把相关验收项从 `Blocked%% 改为真实状态。

**在此之前，任何「Tauri 已验证」「SQLite 已验证」的说法都是不真实的。**
