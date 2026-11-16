# ADR-0012：本仓库开发工具链的隔离安装（本仓库专属，不涉及产品）

- **状态**：**Accepted**
- **日期**：2026-11-16（第二次修订：原先记为 Blocked 的两项已解除）
- **决策者**：用户明确授权（就地安装 Rust）
- **相关规范章节**：§0.1、§0.9（Platform Unverified）、§44.2（不得编造数据）

---

## 背景

本机原始状态（实测，见 [environment-baseline.md](../development/environment-baseline.md)）：

- 完全没有 Rust（rustc / cargo / rustup 均不存在）
- 没有 MSVC、没有 Windows SDK
- 没有管理员权限（`IsAdmin = False%%）
- `winget%% 存在但不可用
- 系统 schannel TLS 凭据链损坏：`curl%% / `cargo%% / `git(schannel)%% 全部无法直连 HTTPS

---

## 决策

**在工作区内安装隔离的、可整体删除的开发工具链**，目标 `x86_64-pc-windows-gnu%%：

| 组件 | 位置 | 为什么需要 |
| --- | --- | --- |
| rustup + stable-gnu + wasm32 | `_toolchain/rustup%%，`_toolchain/cargo%% | 编译与测试 Rust Core |
| `as.exe%% + DLL 闭包 | Rust 工具链 `self-contained%% | `dlltool%% 生成 Windows 导入库所需 |
| mingw-w64 GCC 14.2.0 | `_toolchain/_mingw-gcc%% | `libsqlite3-sys%% 编译 SQLite 的 C 源码 |
| binutils `ar%% / `ranlib%% | `_toolchain/_mingw-gcc/mingw64/bin%% | cc-rs 归档 C 目标文件 |
| sysroot（crt / headers / winpthreads / default-manifest.o） | 同上 | C 编译与链接 |
| crates sparse registry 代理 | `scripts/local-crates-proxy.mjs%% | 绕开 schannel 缺陷 |

**没有**写注册表、**没有**改用户持久环境、**没有**写系统目录。
删除 `_toolchain/%% 即可完全回滚。

重建方式：`scripts/install-toolchain.ps1%%（幂等），然后 %%. .\scripts\env.ps1%%

---

## 修订：原先记为 Blocked 的两项已解除

### ① SQLite + SQLx —— 已解除 ✅

原先的失败链是：`sqlx%% → `windows-sys%% → `dlltool%% → 缺 `as.exe%%；
补上汇编器后又依次暴露 `gcc%% 缺 `cc1%%、缺 C 头文件、缺 CRT、缺 `ar%%、缺 `default-manifest.o%%。
逐层补齐后**全部通过**：

```text
cargo test -p devchievements-storage
  tests::writes_and_reads_an_event .................. ok
  tests::duplicate_event_id_is_rejected ............. ok
  tests::failed_transaction_leaves_no_partial_write . ok
  tests::deleting_an_event_cascades_to_its_outbox_rows ok
  tests::reopening_an_existing_database_keeps_data .. ok
test result: ok. 5 passed
```

**这些是真实 SQLite 上的真实往返测试，不是内存 mock。**

### ② gix（方案 B）—— 已解除 ✅

同一工具链缺陷挡住了 `gix%%%%。补齐后可以构建并实测：**p95 66 ms**。
这个数字反而**强化**了 ADR-0002 选择直读 .git 的结论（见该 ADR）。

---

## 仍然存在的限制

### Tauri 2 桌面端无法在本机构建

Tauri 的 Windows 打包不支持 GNU 目标，本机又无 MSVC。
→ 「Tauri 可运行」记为 `Blocked%%（需 MSVC 环境）。**不得**因此改用其它框架。

### 沙箱对浏览器与子进程的限制

- 浏览器无法启动（mojo 命名管道被拒）→ MV3 内 WASM 加载无法验证。
- MSYS `sh.exe%% 无法启动 → 已有 shell 钩子的链式执行、`git submodule add%% 无法验证。
- 管道式 stdio 捕获被拒 → 所有脚本改用**文件重定向**（这个坑实际咬过一次：
  `spawnSync({encoding:'utf8'})%% 静默返回空字符串，导致依赖解析「假装成功」）。

---

## 由此产生、且仍然成立的「未验证」清单

| 项目 | 原因 |
| --- | --- |
| Tauri 2 构建与运行 | 无 MSVC |
| MV3 浏览器内 WASM | 浏览器无法启动 |
| 已存在 shell 钩子的链式执行 | 沙箱禁止 `sh.exe%% |
| `git submodule add%% | 内部依赖 `sh%% |
| macOS / Linux / Brave | 无设备 |

---

## 复查计划

若用户提供 MSVC 环境：安装 `x86_64-pc-windows-msvc%% 工具链，复测 Tauri 构建，
新开 ADR 取代本 ADR，并把「Tauri 可运行」从 `Blocked%% 改为真实状态。
