# 问题报告：非打包 Win32 进程无法使用 Windows Toast（0x80073D54）

- **日期**：2026-11-16
- **状态**：🔴 **未解决**。已排除代码缺陷，指向环境/策略
- **相关**：[ADR-0015](../decisions/0015-toast-notifications.md)、[ADR-0014](../decisions/0014-desktop-shell-and-msvc.md)

---

## 1. 一句话描述

> 一个**非打包（unpackaged）Rust/Win32 可执行程序**调用
> `ToastNotificationManager.CreateToastNotifierWithId(AUMID)` 一律返回
> `0x80073D54`（该进程没有程序包标识符）。
> **但同一个 AUMID，由 `powershell.exe` 调用则完全正常。**

注册状态已多次核验无误（见 §4）。因此问题不在「注册没做对」，
而在「这个进程不被通知平台接受」。

---

## 2. 环境

| 项 | 值 |
| --- | --- |
| OS | Windows 11 Pro，build **26300** |
| 用户 | 普通用户 **csa，非管理员** |
| 安全软件 | **Kaspersky**（productState 266240）、Windows Defender 同时存在 |
| MSVC | VS Build Tools 18.11.3 / MSVC 14.51.36231 |
| Windows SDK | 10.0.26100.0 |
| Rust | 1.100.0，`x86_64-pc-windows-msvc` |
| windows crate | 0.62.2 |
| WebView2 | 157.0.4341.46 已安装，但本机内嵌 webview 无法启动（故不用自绘弹窗） |

> 补充背景：本会话运行在一个带**文件沙箱**的 agent 环境里。
> 该沙箱拒绝应用进程写 `Start Menu`（`0x80070005`），但 PowerShell 可以写。
> 这解释了为什么注册被拆成「安装器做、应用只读检查」。
> **但这与 0x80073D54 无关**——注册完成后仍然失败。

---

## 3. 决定性的对照实验（请先看这个）

同一个 AUMID `Devchievements.Companion`，从两个进程调用：

| 调用者 | 结果 | 通知中心历史 |
| --- | --- | --- |
| `powershell.exe` | ✅ `Show()` 成功 | **我们的 AUMID +1**，PowerShell 自己 +0 |
| 我们的 Tauri 程序 | ❌ `0x80073D54` | 不变 |
| **裸 Rust 程序**（仅依赖 windows crate，无任何框架） | ❌ `0x80073D54` | 不变 |

历史增量的实测输出：

```text
delta   ours=1  powershell=0
```

**这两条一起排除了两件事**：

1. AUMID 注册是对的——否则 PowerShell 不可能记到我们名下；
2. 问题不在 Tauri / 我们的应用结构——**一个只调用该 API 的裸程序同样失败**。

---

## 4. 已完成的注册（每一项都独立核验过）

### 4.1 开始菜单快捷方式

```text
路径   : %APPDATA%\Microsoft\Windows\Start Menu\Programs\Devchievements.lnk
存在   : True
目标   : ...\apps\desktop\src-tauri\target\debug\devchievements-desktop.exe
AUMID  : Devchievements.Companion     ← 由 Shell 属性系统读出
```

读取方式（不是读二进制，是问 Shell）：

```powershell
$sh = New-Object -ComObject Shell.Application
$i  = $sh.Namespace($programs).ParseName('Devchievements.lnk')
$i.ExtendedProperty('System.AppUserModel.ID')   # -> Devchievements.Companion
```

生成方式：`ShellLink` + `IPropertyStore::SetValue(PKEY_AppUserModel_ID)`
（手工构造 `PROPVARIANT`：`VT_LPWSTR` + `CoTaskMemAlloc`）+ `IPersistFile::Save`。

### 4.2 注册表

```text
HKCU\Software\Classes\AppUserModelId\Devchievements.Companion
  DisplayName = Devchievements
```

### 4.3 通知平台的注册表（**这一条很关键**）

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\Devchievements.Companion
  LastNotificationAddedTime = 134393148185489498
  PeriodicNotificationCount = 2
```

即：**通知平台是认识这个 AUMID 的**，并且确实收过它的通知（那 2 条是 PowerShell 发的）。

### 4.4 应用侧自检

```text
$ devchievements-desktop.exe --toast-status
{"detail":"registered","ready":true}     (exit 0)
```

---

## 5. 已排除的假设（附方法与结果）

| # | 假设 | 做法 | 结果 |
| --- | --- | --- | --- |
| 1 | 注册表键不够 | 只写键，不建快捷方式 | ❌ 仍失败 |
| 2 | 缺进程级 AUMID | 加 `SetCurrentProcessExplicitAppUserModelID` | ❌ 仍失败 |
| 3 | 去掉进程级 AUMID 反而好 | 代码里删掉该调用后重建再测 | ❌ 仍失败 |
| 4 | 快捷方式属性没写对 | Shell 属性系统读回 | ✅ 正确，不是问题 |
| 5 | Shell 没刷新，平台用缓存 | `SHChangeNotify(SHCNE_ASSOCCHANGED)` | ❌ 仍失败 |
| 6 | 是 Tauri 的问题 | 写**裸 Rust** 最小程序只调该 API | ❌ 同样失败 |
| 7 | 是 PowerShell 的错觉（其实用的它自己身份） | 对比两个 AUMID 的历史增量 | ✅ 确实记在我们名下，假设不成立 |
| 8 | 用户关了我们的通知 | 查 `Notifications\Settings` | 有条目，无 `Enabled=0` |

---

## 6. 我的假设（**均未验证**，按可能性排序）

### H1：安全软件拦截未签名可执行程序（我认为最可能）

依据：

- 机器上装有 **Kaspersky**，且应用进程连写 `Start Menu` 都被拒（`0x80070005`），
  而 PowerShell 写同一目录正常 —— 这是典型的「按进程信誉拦截」表现；
- 裸 Rust 程序（全新未签名二进制）与 Tauri 程序表现一致；
- `powershell.exe` 是系统签名程序，正常。

**验证方法**：

1. 临时退出 Kaspersky，重跑 `toastprobe.exe`；
2. 或给 exe 做代码签名（哪怕自签名并信任）后再测；
3. 或查 Kaspersky 的「应用程序控制 / 入侵防御」日志有没有拦 `devchievements-desktop.exe`。

### H2：Windows 11 build 26300 收紧了非打包应用的限制

依据：26300 是很新的构建号。微软在较新版本上更强调 MSIX / 稀疏包身份。

**验证方法**：在一台回归到 22631 / 26100 的机器上跑同一个 `toastprobe.exe`。

### H3：必须**从快捷方式启动**才绑定身份

依据：文档强调快捷方式，未明确是否要求「由快捷方式启动」。
我试过一次 `Start-Process $lnk`，但当时用的是**旧二进制**（还不认识命令行参数），
**那次实验无效**，不能作为证据。

**验证方法**：重建后，从开始菜单真实点击启动，再触发一次 toast。

### H4：非管理员账户的限制

**验证方法**：用管理员账户跑同一个 `toastprobe.exe`。

---

## 7. 复现步骤（最小）

```text
# 1. 生成一个只调用 Toast API 的裸程序（本仓库里有：_toastprobe/）
cd _toastprobe
C:\...\_toolchain\cargo\bin\cargo.exe +stable-x86_64-pc-windows-msvc build

# 2. 直接运行 -> 失败
.\target\debug\toastprobe.exe Devchievements.Companion bare-probe
# BARE TOAST FAILED ... error=CreateToastNotifierWithId: ... (0x80073D54)

# 3. 同一个 AUMID 由 PowerShell 调用 -> 成功
powershell -Command "[void][Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]; <...>"
```

`_toastprobe/src/main.rs` 只有约 40 行，除 `windows` crate 外无任何依赖。

---

## 8. 需要对方回答的问题

1. **非打包 Win32 应用要显示 Toast，除了「注册表 + 带 AUMID 的快捷方式」之外，
   还有别的必要条件吗？**（例如：签名、安装位置、从快捷方式启动、清单文件）
2. `0x80073D54` 在**已注册 AUMID 且通知平台已收录该 AUMID** 的情况下，
   还可能由什么原因触发？
3. 有没有办法在不加签名、不加 MSIX 的前提下让任意本地 exe 获得通知身份？
4. 如果必须 MSIX / 稀疏包（sparse package），对一个开源本地优先应用，
   最轻量的做法是什么？（是否必须签名？）

---

## 9. 相关文件

| 文件 | 内容 |
| --- | --- |
| `apps/desktop/src-tauri/src/toast.rs` | Toast 实现（注册快捷方式 + 显示） |
| `apps/desktop/src-tauri/src/main.rs` | `--toast-probe` / `--toast-status` / `--register-shortcut` / `--notify-shell` |
| `scripts/install-companion.ps1` | 安装器：写快捷方式 + 注册表 + 通知 shell + 自校验 |
| `scripts/uninstall-companion.ps1` | 反向清理 |
| `scripts/e2e/toast.mjs` | 端到端验证（0=通过 / 1=坏了 / 2=环境阻止验证） |
| `_toastprobe/` | 最小复现程序（不在 git 里，可现场生成） |

---

## 10. 当前结论

**产品侧代码是完整的**：注册、自检、显示、失败降级都已实现并有测试；
安装器实测 exit 0，快捷方式与注册表都经过独立核验。

**但在这台机器上无法验证 Toast 真的弹出**，且已证明不是我们的代码问题
（裸程序同样失败，而系统进程用同一 AUMID 成功）。

因此该项在 `STATUS.md` 中记为 `Implemented` 而非 `Tested`；
`scripts/e2e/toast.mjs` 以退出码 **2**（环境阻止验证）报告，而不是假装通过或假装失败。
