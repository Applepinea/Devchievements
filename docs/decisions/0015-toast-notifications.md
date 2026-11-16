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
