# 问题报告 v2：非打包 Win32 进程无法使用 Windows Toast（0x80073D54）

- **日期**：2026-11-16（v2，补充了决定性测量）
- **状态**：🔴 **未解决**
- **v1 → v2 的变化**：v1 把原因归给「安全软件拦截」，这是在没有测量的情况下下的结论。
  v2 补充了进程身份的实际测量，**推翻了一个关键假设**，并把结论收窄为可检验的问题。

---

## 0. 最重要的一条：PowerShell 也没有 Package Identity，但它能发 Toast

对两个进程实际调用 `GetCurrentPackageFullName`：

| 进程 | 返回值 | 能否发 Toast |
| --- | --- | --- |
| `powershell.exe` 5.1 | **15700 `APPMODEL_ERROR_NO_PACKAGE`** | ✅ 能（且记在我们的 AUMID 名下） |
| 我们的 Rust 程序 | **15700 `APPMODEL_ERROR_NO_PACKAGE`** | ❌ 不能 |

```text
PowerShell process package identity: ABSENT rc=15700 (APPMODEL_ERROR_NO_PACKAGE)
Rust  process: package identity: ABSENT  code=WIN32_ERROR(15700)
```

**因此「进程缺少 Package Identity」无法解释二者的差异**：两者都缺。
`0x80073D54` 是本次失败的**错误码**，但它不是**充分原因**——
同一个错误码描述的「缺身份」状态，在 PowerShell 里同样成立却成功。

> 这一条纠正了我方与协助方的共同假设。它不否定「换成 AppNotificationManager」
> 这条建议的价值，但说明**不能**把 0x80073D54 当作「必须获得 Package Identity」的证据。

---

## 1. 全部实测数据

### 1.1 注册状态（都正确）

| 项 | 值 | 来源 |
| --- | --- | --- |
| 快捷方式 | 存在 | 文件系统 |
| 快捷方式目标 | `...\devchievements-desktop.exe` | Shell 属性 |
| `System.AppUserModel.ID` | `Devchievements.Companion` | Shell 属性（不是读二进制） |
| `HKCU\...\AppUserModelId\Devchievements.Companion\DisplayName` | `Devchievements` | 注册表 |
| `HKCU\...\Notifications\Settings\Devchievements.Companion` | 存在，`PeriodicNotificationCount = 2` | 注册表 |
| Shell 应用列表 `Get-StartApps` | **列出 `Devchievements` / `Devchievements.Companion`** | Shell |
| 应用自检 `--toast-status` | `{"ready":true}` | 应用 |

即：**Shell 与通知平台都认识这个应用**。

### 1.2 调用结果（都失败，且两个重载失败方式不同）

```text
attempt 1 (CreateToastNotifierWithId(aumid)) : 0x80073D54  该进程没有程序包标识符
attempt 2 (SetCurrentProcessExplicitAppUserModelID 先设, 再 CreateToastNotifier())
                                             : 0x80070490  找不到元素
```

两个错误码不同：

- `0x80073D54` = `APPMODEL_ERROR_NO_PACKAGE`
- `0x80070490` = `ERROR_NOT_FOUND` —— **平台按进程身份去查，查不到**

### 1.3 已排除的变量

| 变量 | 做法 | 结果 |
| --- | --- | --- |
| 是否 Tauri 的问题 | 40 行裸 Rust 程序，仅依赖 `windows` crate | ❌ 同样失败 |
| 是否要在设置 AUMID 之前调用 WinRT | 改为**最前面**设置，再调用 | ❌ 仍然失败 |
| 是否必须从快捷方式启动 | 把探针替换为快捷方式目标，经 `start <lnk>` 启动 | ❌ 仍然失败（且日志证明它确实是被快捷方式拉起的） |
| 是否 Smart App Control | `VerifiedAndReputablePolicyState` | **0 = 关闭**，排除 |
| 是否注册表/快捷方式没写对 | Shell 属性 + `Get-StartApps` 双向验证 | ✅ 都对 |

### 1.5 两个新实验（**关键**）

#### 实验 A：同一个 .NET Framework 运行时下的 C# 程序

用 `csc.exe` 编译一个 20 行的 C# 控制台程序，调用同一个 `CreateToastNotifier(aumid)`：

```text
CS TOAST FAILED: InvalidOperationException: 该进程没有程序包标识符 (0x80073D54)
```

**这否定了「运行时/激活路径不同」的假设**：PowerShell 5.1 与这个 C# 程序用的是
同一个 .NET Framework 4.x、同一套 WinRT 投影、同一个 API 重载。

#### 实验 B：给同一个程序加代码签名

生成自签名证书 → 加入 `CurrentUser\Root` 与 `TrustedPublisher` → `signtool` 签名
（`Get-AuthenticodeSignature` 显示 **Valid**）→ 重跑：

```text
CS TOAST FAILED: InvalidOperationException: 该进程没有程序包标识符 (0x80073D54)
```

**签名存在本身不够**。注意：这是**自签名**，不是公有信任链上的签名。

### 1.6 排除矩阵（每一条都是实测）

| 变量 | 是否可解释差异 |
| --- | --- |
| API 选择（WinRT Toast） | ❌ PowerShell 用同一个 API 成功 |
| 运行时（.NET Framework） | ❌ C# 用同一运行时失败 |
| Package Identity | ❌ 两者都没有（15700） |
| 快捷方式 / AUMID / 注册表 | ❌ 全部正确且被 Shell 承认 |
| 从快捷方式启动 | ❌ 实测无效 |
| Smart App Control | ❌ 已关闭 |
| 有签名（自签名） | ❌ 无效 |
| **发布者信任（公有 CA / 微软签名）** | ⬜ **唯一尚未排除** |
| 位置（System32 vs 用户目录） | ⬜ 未单独测试 |

### 1.4 仍然存在的差异

| 项 | PowerShell | 我们的 exe |
| --- | --- | --- |
| Authenticode 签名 | **Valid**（微软） | **NotSigned** |
| 位置 | `C:\Windows\System32\...` | `C:\Users\csa\Documents\...` |
| 运行时 | .NET Framework 4.x WinRT 投影 | windows-rs（`RoActivateInstance`） |
| Package Identity | 无 | 无 |
| AUMID 使用 | 可用我们的 | 不可用 |

---

## 2. 目前的假设（已按新实验重排）

### H-1：发布者信任（**唯一未被排除的解释**）

实测矩阵只剩这一列没打钩。PowerShell 是微软签名的系统组件；
我们的程序即使**自签名并被本机信任**仍然失败，说明平台要求的可能是
「被系统/安全软件认可的发布者」，而不是「文件上有签名」。

**注意**：这仍是假设。错误码指向身份而非访问拒绝，与「信誉拦截」的典型表现不完全吻合。

**验证办法**：用公有 CA 证书签名后重测；或临时停用 Kaspersky 后重测。

### H-2：位置（System32 vs 用户目录）

**验证**：把探针放到 `%ProgramFiles%` 或 System32 下运行。

### H-3：Windows 11 build 26300 的行为

**验证**：换一台 22631 / 26100 的机器跑同一个探针。

### 已被实验否定的假设

| 假设 | 否定它的实验 |
| --- | --- |
| 必须是打包应用 / 需要 Package Identity | 双方都无 Package Identity，PowerShell 仍成功 |
| 是 Tauri 或 Rust 的问题 | 裸 Rust 程序同样失败 |
| 是 .NET 与 windows-rs 的激活路径差异 | C# 在同一 .NET Framework 下同样失败 |
| 只要签名就行 | 自签名 + 本机信任后仍失败 |
| 必须从快捷方式启动 | 从快捷方式启动实测仍失败 |
## 3. 关于 AppNotificationManager 的可行性（已核实）

| 项 | 结论 |
| --- | --- |
| `windows` crate 0.62 是否含 AppSDK 绑定 | ❌ **不含**（只有 `Windows` 命名空间，无 `Microsoft`） |
| 本地 sparse registry 是否有 AppSDK 的 Rust 绑定 | ❌ `windows-app-sdk` / `winappsdk` 均 404 |
| Windows App Runtime 是否已安装 | ✅ 有 1.5 / 1.6 / 1.7 / 1.8 / 2.x 多个版本 |

**含义**：从 Rust 用 `AppNotificationManager` 需要自己声明 AppSDK 的 COM 接口
（`windows_core::interface!` + 手工激活工厂），是实打实的工作量，不是换个函数名。

建议路径：

1. 先用 **C# + AppSDK 投影**验证「换 API 是否真的能解决」，成本低；
2. 确认有效后，再投入 Rust 绑定；
3. 若 C# 也失败，说明问题不在 API 选择，回到 H-A/H-B。

---

## 4. 对项目的影响与建议状态

| 项 | 建议状态 |
| --- | --- |
| AUMID 注册 / 开始菜单快捷方式 / 安装器 | `Implemented`（实测 exit 0） |
| 传统 Toast API 调用 | `Implemented`，**运行验证未通过** |
| Toast 端到端 | `Platform Unverified` |
| AppNotificationManager 路线 | `Not Started` |
| Sparse Packaging | `Not Started` |

**同意协助方的判断：这不阻塞整个项目。** 通知呈现是可替换层，
`devchievements-notifications` 的合并与状态机逻辑与呈现方式无关，已独立验证。

---

## 5. 下一步（建议顺序，已按新数据调整）

| # | 实验 | 目的 |
| --- | --- | --- |
| 1 | `csc.exe` 编译 C# 控制台，调用同一 API | 区分「运行时/激活路径」与「二进制信誉」 |
| 2 | 给探针自签名并信任后重测 | 验证 H-B |
| 3 | C# + AppSDK `AppNotificationManager` | 验证换 API 是否解决 |
| 4 | 另一台 Windows 11 | 验证 H-C |
| 5 | Sparse Packaging | 若确需 Package Identity |

---

## 6. 复现与文件

- 最小探针：`_toastprobe/`（40 行，仅依赖 `windows` crate，输出身份与两种调用的结果）
- 构建：`cargo +stable-x86_64-pc-windows-msvc build`（需 MSVC）
- 应用侧：`apps/desktop/src-tauri/src/toast.rs`
- 安装器：`scripts/install-companion.ps1`
- 端到端：`scripts/e2e/toast.mjs`（0=通过 / 1=坏了 / **2=环境阻止验证**）
