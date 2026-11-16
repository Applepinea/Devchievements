# Phase 2b 进展（完整 Desktop Only 产品）

- **日期**：2026-11-16
- **状态**：**In Progress**
- **前置**：用户已明确指示进入 Phase 2b（跨过检查点 A）

---

## 0. 跨检查点时的已知未验证项

按 AGENTS.md §10，这里记录**用户批准跨越时仍未通过验证的东西**，
不能因为它们「被接受了」就当作已完成：

| 项 | 状态 |
| --- | --- |
| 自定义通知弹窗（WebView2） | ❌ 本机无法启动 |
| 系统 Toast 端到端 | ❌ 平台拒绝（见 [报告](toast-identity-blocker.md)） |
| 设置窗口 / 图鉴等全部页面 | ❌ 依赖 WebView2 |
| shell 钩子链式执行 | ❌ 本机 `sh.exe` 无法启动 |
| 实时 IPC 通道 | ⚠️ 沙箱拒绝命名管道 |

**Phase 2b 的大部分交付物是 UI**，因此在解决 WebView2 之前，
我只能做其中不依赖渲染的部分，并明确标注其余为阻塞。

---

## 1. 本轮：平台能力探测与降级链（§29）

### 1.1 为什么先做这个

§29.1 要求「启动时探测平台能力，按降级链选择展示方式」。
更直接的理由：**它正好是当前 Toast 问题的产品化解法**——
产品不该假设自己能弹窗，而该知道当前能做什么，并按链路降级。

### 1.2 实现

`crates/platform`，13 个测试。

五种状态严格按 §29.1：
`Native` / `Partial` / `DesktopSpecific` / `NotificationFallback` / `Unverified`。

**关键规则**：只有 `Native` 与 `Partial` 算「可用」。
`Unverified` 不等于「大概能用」——把它当可用正是 §29.1 警告的失败模式。

降级链：

```text
桌面浮窗 → 系统通知 → 仅应用内提示（图鉴新解锁标记）
```

全屏策略（§29.5）三种：降级为系统通知（默认）/ 延后 / 静默。

### 1.3 运行时行为改变（这是个真实改进）

以前：每个事件都调用一次 Toast，每次失败都打一行错误。

现在：首次调用即探测，记住结果，之后直接走降级路径。

实测日志：

```text
NOTIFY unavailable tag=first_commit reason=CreateToastNotifierWithId: 0x80073D54
NOTIFY degraded channel=in-app-only (spec 29.1 fallback chain)
NOTIFY skipped tag=daily:uncommon channel=in-app-only
```

注意第二条通知**没有再尝试**。这在正常机器上会自动变成 `Native`，
因为探测结果来自真实调用而不是硬编码。

### 1.4 实测能力快照

```json
{
  "capabilities": {
    "desktop_popup": "notification_fallback",
    "system_notification": "unverified",
    "tray": "native",
    "do_not_disturb_detection": "unverified",
    "dpi_scaling": "unverified",
    "multi_monitor": "unverified",
    "wasm": "unverified"
  },
  "channel": "in_app_only",
  "summary": "popup=notification-fallback notification=unverified tray=native"
}
```

只有 `tray` 是 `native`——因为托盘图标确实建起来了、进程确实活着。
其余一律 `unverified`，包括本可以吹成「支持」的那些。

### 1.5 命令

| 命令 | 作用 |
| --- | --- |
| `--capabilities` | 打印能力集与降级结果 |
| `--diagnostics` | 快照中已包含 `capabilities` |
