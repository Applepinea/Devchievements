# ADR-0015：通知呈现改用 Windows Toast（安装器注册身份）

- **状态**：**Accepted**
- **日期**：2026-11-16
- **相关规范章节**：§16.4、§16.6、§28、§32.1
- **前置**：[ADR-0014](0014-desktop-shell-and-msvc.md)

---

## 背景

规范设想 Tauri 自绘通知窗口，但本机内嵌 WebView2 无法启动（ADR-0014）。
实测确认 Windows 原生 Toast 可以正常弹出，因此改走 Toast。

## 决策一：呈现用 Toast

§16.4/§16.6 规定的是**行为**（普通提交只出声、多奖励合并为一个弹窗），
没有规定必须自绘。Toast 还自带系统勿扰设置（§16.6 要求），且不依赖 WebView2。

**代价**：§28 的自定义动画与皮肤无法用 Toast 实现，那部分仍未完成。

## 决策二：注册归安装器，应用只读检查

非打包进程要用 Toast，必须同时具备：

1. 开始菜单快捷方式，属性 `System.AppUserModel.ID` 指向 AUMID；
2. `HKCU\Software\Classes\AppUserModelId\<id>` 提供显示名。

这两件事都写系统位置。**应用每次启动都去改开始菜单和 HKCU 是不可接受的**：
行为上像恶意软件，权限上也常常不被允许。

所以职责分开：

| 角色 | 职责 | 接口 |
| --- | --- | --- |
| `scripts/install-companion.ps1` | 写快捷方式、写注册表、通知 shell、自校验 | 调用应用的 `--register-shortcut` / `--notify-shell` |
| `scripts/uninstall-companion.ps1` | 清理快捷方式与注册表；**不动用户数据** | — |
| 应用 | 只读检查 | `--toast-status`（JSON，未就绪退出码 3） |

安装器成功时输出（实测）：

```text
install: System.AppUserModel.ID = [Devchievements.Companion]
{"detail":"registered","ready":true}
install: done
```

## 验证状态

🔴 **本机未能验证 Toast 本身。**

| 步骤 | 结果 |
| --- | --- |
| 生成带 AUMID 的快捷方式 | ✅ Shell 属性系统读得到 |
| 装入开始菜单 | ✅（由安装器，exit 0） |
| 应用读到注册状态 | ✅ `{"ready":true}` |
| **实际弹出 Toast** | ❌ 仍 `0x80073D54` |
| 以 PowerShell 自身身份弹出 | ✅ 对照实验成功 |

对照实验说明：**Toast 机制本身可用，缺的是这个进程被平台信任**。
具体原因未查明（怀疑安全软件，未验证）。

## 复查计划

在普通 Windows 机器上：

```text
node scripts/verify.mjs
powershell -File scripts/install-companion.ps1
node scripts/e2e/toast.mjs        # 退出码 0 才可把 STATUS 改成 Tested
```

---

## 2026-11-17 复测：结论被推翻

### 本轮测得的硬事实

| 测量 | 结果 |
| --- | --- |
| `msedgewebview2.exe --version` 退出码 13 | **与 Toast 无关**（该参数不被支持） |
| 快捷方式 target | 正是当前 exe，且 `Get-StartApps` 读到 `Devchievements.Companion` |
| 注册表 `HKCU\Software\Classes\AppUserModelId` | 存在，DisplayName 正确 |
| 进程 AUMID | `GetCurrentProcessExplicitAppUserModelID` = `Devchievements.Companion` |
| `GetTemplateContent` | 成功（WinRT 本身可用） |
| `CreateToastNotifierWithId(ours)` | `0x80073D54` |
| **`CreateToastNotifierWithId(Microsoft.Windows.Explorer)`** | **同样 `0x80073D54`** |
| `CreateToastNotifier()`（无参重载） | `0x80070490` |
| 从开始菜单快捷方式启动 | 无差别 |
| STA 公寓（`CoInitializeEx` 返回 S_OK） | 无差别 |
| **PowerShell 用我们的 AUMID 弹窗** | **成功** |

### 这推翻了什么

旧文档写的是「缺快捷方式导致 `0x80073D54`」。
**这个因果是错的**：快捷方式完全正确，失败依旧。

决定性证据是 **Explorer 的系统 AUMID 也失败**——
如果问题在注册，系统自带的标识不可能也失败。

所以失败点在**进程**，不在标识。

### 因此也没有「修好注册就能好」这回事

ADR-0015 的结论「注册齐了就成功」不再成立。
在找到进程级差异之前，**不能声称修好了注册就能弹窗**。

### 仍未排除的方向

1. **应用清单缺 `maxversiontested`**：微软对未打包应用的文档要求该项，
   Rust MSVC 默认清单不含它。这是目前最有说服力的剩余猜想。
2. **Windows App SDK 的 `AppNotificationManager`**：官方为未打包应用准备的正路，
   但需要引入 AppRuntime 引导器（见 2a.16）。

### 状态

Toast 端到端仍为 **未验证**。`node scripts/e2e/toast.mjs` 退出码 2（环境阻止验证）。
降级链中间档在本机不可用，产品侧已按 §29.1 降级为仅应用内提示。

---

## 附：`maxversiontested` 尝试（未成功，已停止）

写了 `app.manifest` 并通过 `RUSTFLAGS` 传 `/MANIFESTINPUT`，
链接失败：

```text
LINK : fatal error LNK1123: 转换到 COFF 期间失败
```

原因：rustc 的 MSVC 链接行**本身已带 `/MANIFEST:EMBED`**，
再叠一个 `/MANIFESTINPUT` 会产生重复的清单资源。

正规做法是用 `embed-resource` 在 `build.rs` 里生成资源文件覆盖默认清单。
本次没有继续，原因是前面每一项猜想都花了完整构建周期且全部否证，
继续投入的收益不确定。

**这里停下来是判断，不是借口**：如果之后要再试，起点就是这条——
用 `embed-resource` 换掉 rustc 的默认清单，而不是叠加。
