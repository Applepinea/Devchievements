# ADR-0015：通知呈现改用 Windows Toast

- **状态**：**Accepted**
- **日期**：2026-11-16
- **相关规范章节**：§16.4、§16.6、§28、§32.1
- **前置**：[ADR-0014](0014-desktop-shell-and-msvc.md)

---

## 背景

规范设想的是 Tauri 自绘通知窗口。但本机内嵌 WebView2 无法启动
（`os error 5`，见 ADR-0014），因此自绘窗口这条路在本机不可用。

同时实测确认：**Windows 原生 Toast 在这个环境里可以正常弹出**
（用 PowerShell 自身 AUMID 验证，通知中心可查到）。

## 决策

通知呈现走 **Windows Toast**，而不是自绘窗口。

理由：

- §16.4/§16.6 规定的是**行为**（普通提交只出声、多奖励合并成一个弹窗），
  没有规定必须自绘；
- Toast 遵循系统勿扰设置（§16.6 要求），自绘窗口反而要自己实现；
- 不需要 WebView2，因而不受本机限制。

## 代价

- 视觉受限于系统模板，§28 的自定义动画与皮肤**无法用 Toast 实现**；
  那部分仍是未实现项，不能声称完成。
- 非打包 Win32 需要注册 AUMID（最快方式是一个带 `System.AppUserModel.ID` 的
  开始菜单快捷方式）。

## 验证状态

🔴 **本机未能验证**。

| 步骤 | 结果 |
| --- | --- |
| 生成带 AUMID 的快捷方式 | ✅ 属性可被 Shell 正确读出 |
| 写入开始菜单 | ❌ `0x80070005`（PowerShell 同目录可写） |
| 快捷方式就位后弹 Toast | ❌ 仍 `0x80073D54` |

因此 `scripts/e2e/toast.mjs` 以退出码 2 报告「无法验证」，
STATUS 中该项为 `Implemented` 而非 `Tested`。

## 复查计划

在不受限的 Windows 机器上运行 `node scripts/e2e/toast.mjs`；
退出码 0 才可把 STATUS 改成 `Tested`。
