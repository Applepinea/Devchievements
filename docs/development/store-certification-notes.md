# Partner Center 认证备注（Store 提交用）
> 用途：提交 MSIX 时，贴进 Partner Center 的 **Notes for certification** 字段。
> 语言：英文（审核人员按英文看）。下面分「可直接粘贴版」和「逐条说明」两部分。

---

## 1. 可直接粘贴的版本

```text
DEPENDENCY: Microsoft Edge WebView2 Runtime (Evergreen).

The app needs the WebView2 Runtime to render its window. On Windows 11 it is
preinstalled; on Windows 10 it is normally present as well.

If the runtime is missing, the app does NOT crash and does NOT show a blank window:

  1. The main process starts and detects the missing runtime before creating any
     window. Detection reads the EdgeUpdate registry value for the WebView2 client
     GUID {F3017226-FE2A-4295-8BDF-00C3A9A7E4C5} and then verifies that
     msedgewebview2.exe actually exists on disk, because an image can carry the
     registry value without the runtime files.
  2. A native Windows message box appears immediately (bilingual, English and
     Simplified Chinese) stating that the WebView2 Runtime is required.
     - Yes: the app runs the official Microsoft Evergreen bootstrapper that is
       shipped inside this package, re-checks the runtime, and continues starting.
       No restart is needed. If the bootstrapper cannot repair it (for example with
       no network), the official download page opens instead and the process exits
       with code 2.
     - No: the official download page opens in a browser and the process exits with
       code 2.

Exit code 2 is deliberate: it lets a missing dependency be told apart from a crash.

The app declares no ExternalDependency on WebView2, and does not rely on the Store
installing it.

OTHER NOTES

- The app runs as a full-trust Win32 application (runFullTrust). It writes Git hook
  files into repositories the user explicitly adds, and copies its own helper
  executable from the package into %LOCALAPPDATA%\devchievements\bin so that git
  can execute it; executables inside WindowsApps cannot be started by a process
  without the package identity.
- User data is stored in %LOCALAPPDATA%\devchievements and is not removed on
  uninstall.
- The app does not send data anywhere by default. No telemetry, no network calls
  except the version check described above.
- Startup is disabled by default (manifest startupTask Enabled="false").
```

---

## 2. 每句话为什么这么写

| 写法 | 原因 |
| --- | --- |
| 先给 GUID 再给行为 | 审核人员会去核对注册表；给出具体键路径能省掉一轮往返 |
| 强调「先检测、后建窗口」 | 这正是「功能不可用」的判定点：白屏或崩溃才会被拒 |
| 写明退出码 2 | 让「进程退出」与「进程崩溃」可以区分，日志或事件查看器能对上 |
| 不宣称 ExternalDependency | 那个声明实测会被忽略，写了反而制造错误预期 |
| 主动交代复制 helper 的行为 | 审核看到「往用户目录写 exe」会警觉；先解释原因，比被问再答好 |
| 写明数据目录与卸载行为 | 商店对「卸载后残留」有明确要求，主动说明可控 |

---

## 3. 提交前自查

- [ ] 干净 Windows 10 虚拟机（**未跑过 Windows Update**）上装包并启动，确认弹出原生框而不是白屏
- [ ] 在同一台机器上点「是」，确认运行时装好后程序**继续启动**，不需要重启
- [ ] 断网点「是」，确认 bootstrapper 失败后回落到打开下载页，且退出码为 2
- [ ] 认证备注里提到的路径与实际一致（`%LOCALAPPDATA%\devchievements`）
- [ ] 包内确实带 `webview2-setup.exe`（`makeappx` 输出里能看到）

---

## 4. 已知不接受的做法（避免临时改方案）

| 做法 | 结果 |
| --- | --- |
| MSIX 清单里声明 `ExternalDependency` | 实测被忽略，不装任何东西 |
| 让应用在缺运行时直接退出 | 可能被判「功能不可用」 |
| 依赖 wry 自己的错误框 | 是英文且不提供修复路径，用户无法自救 |
