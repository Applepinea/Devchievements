# ADR-0014：桌面外壳（Tauri 2）与 MSVC 工具链

- **状态**：**Accepted**
- **日期**：2026-11-16
- **相关规范章节**：§3、§24、§25、§32.1
- **前置**：[ADR-0012](0012-isolated-dev-toolchain.md)（隔离工具链）

---

## 背景

Phase 0 起，Tauri 一直是 **Blocked**：本机没有 MSVC，而 Tauri 在 Windows 上
必须用 MSVC 工具链链接。

现在 MSVC 装好了（VS Build Tools 18.11.3，MSVC 14.51.36231，Windows SDK
10.0.26100.0），但**不是装好就能用**，实测遇到三个坑，全部记在这里。

---

## 决策

### 1. 桌面应用是**独立的 cargo 项目**，不是 workspace 成员

`apps/desktop/src-tauri` 被写进 `[workspace] exclude`。

原因：一个 target 目录不能同时放 MSVC 与 GNU 的产物。仓库其余部分在这台机器上
用 GNU 构建（ADR-0012），Tauri 必须用 MSVC。放进同一个 workspace 会让
`cargo build --workspace` 在其中一个工具链下必然失败。

代价：`verify.mjs` 需要单独一步构建它（已加 `desktop-build`）。

### 2. 用 `interprocess` 之外的**原生**方式获取 MSVC 环境

`scripts/dev/build-desktop.bat` 自己调用 `vcvarsall.bat`，
**不依赖调用者预先 source 过什么**。

实测教训：`verify.mjs` 为 Rust 步骤重制了 PATH 并设置 `CC=mingw gcc`，
把这套环境传给 .bat 会让 `vcvarsall` 的 SDK 探测失效，
表现为 `windows.h` 与 `malloc.h` 找不到（但 MSVC 自己的头文件在）。
所以桌面构建步骤使用**原样 environment**。

### 3. 应用默认**只建托盘、不建窗口**

不是权宜之计：§32.1 要的就是托盘常驻 Companion，设置窗口是用户主动打开的。
好处是数据回路与通知不依赖内嵌 webview。

---

## 本机踩到的三个坑（都有实测证据）

### 坑 1：Git 的 `link.exe` 抢在 MSVC 前面

`C:\Users\<user>\AppData\Local\hermes\git\usr\bin\link.exe` 是 GNU 的
`link` 工具，不是链接器。它在 PATH 里排在 MSVC 前面，于是"链接器找不到"之类的
错误会指向完全错误的方向。

`scripts/env-msvc.ps1` 把 MSVC 的 bin 目录前置，并在发现解析到 Git 的
`link.exe` 时**明确告警**。

### 坑 2：批处理里 `(x86)` 会提前闭合 `if` 块

```batch
if defined X (
  echo %PATH%        REM 路径里的 (x86) 让这个块在这里就结束了
)
```

报错是 `"\Microsoft was unexpected at this time."`，与真实原因毫无关系。
规则：**任何含路径的东西都不要放进括号块**。

### 坑 3：WebView2 在本沙箱内无法启动

- WebView2 运行时**已安装**（157.0.4341.46）；
- 但 `msedgewebview2.exe --version` 直接退出，码 13、无输出；
- Tauri 建窗口时 panic：`Failed to setup app: 拒绝访问。(os error 5)`。

与 Phase 0 的 `sh.exe`、以及命名管道的失败同源：沙箱拒绝 Chromium 的 mojo IPC。

**关键区分（已实测）**：普通 Win32 窗口是**允许**的（WinForms 探针成功创建并显示）。
所以限制不是"不能有窗口"，而是"不能有 WebView2"。

---

## 验证结果

| 项 | 结果 |
| --- | --- |
| Tauri 2.12.6 + wry 依赖树完整编译 | ✅ |
| MSVC 链接（`link.exe` 为真 MSVC） | ✅ 可执行文件运行正常 |
| 托盘应用启动 | ✅ `TRAY READY`，进程存活 |
| 真实提交驱动托盘应用 | ✅ 见 `scripts/e2e/desktop-tray.mjs` |
| 合并弹窗 + 普通提交只出声 | ✅ 实测输出 |
| **内嵌 webview 窗口** | ❌ **本沙箱不可运行** |
| **Windows Toast** | ❌ 非打包进程无包标识（0x80073D54），与本沙箱无关 |

---

## 后果

- 桌面外壳**已可构建、可运行、可驱动真实数据回路**。
- **视觉弹窗（自定义通知窗）仍未验证**，因为需要 WebView2。
- 这不是代码缺陷：同一份二进制在不受沙箱限制的机器上应当正常。但按 AGENTS.md §10，
  **在真机验证之前不得声称可用**，所以状态是 `Partially Implemented`。

## 复查计划

在不受限的 Windows 机器上运行 `devchievements-desktop.exe`，
确认设置窗口能打开，并把结果补进兼容性矩阵。

---

## 🔴 更正（2026-11-17）：WebView2 其实可以运行

**本文档此前断言「本机内嵌 WebView2 无法启动」，这个结论是错的。**

真正的失败原因是：**WebView2 的用户数据目录不可写**，而
`CreateCoreWebView2EnvironmentWithOptions` 只把结果报成 `E_ACCESSDENIED`（`os error 5`），
看不出是哪一步被拒。

修法是一行：给窗口显式指定数据目录。

````rust`
WebviewWindowBuilder::new(app, "main", url)
    .data_directory(data_dir.join("webview"))   // <- 关键
    .build()?;
`````

实测：`node scripts/e2e/window.mjs` → **4 项全过**，窗口正常创建。

### 我为什么会判断错

我用 `msedgewebview2.exe --version` 退出码 13 当作证据——
但那个参数对 WebView2 运行时并不支持，**退出码 13 与「不能运行」无关**。
而 Edge 浏览器本身在这个沙箱里是完全正常的（`--version` 与 headless 都成功），
这条反证当时就在手边，我没有去测。

教训：**用一个未经验证的失败信号去解释另一个失败**，会把两个不相干的事连起来。
