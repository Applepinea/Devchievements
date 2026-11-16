# ADR-0016：Windows 通知 API 的选择与兼容性边界

- **状态**：**Proposed**（API 选型未定稿，受未解决的环境问题影响）
- **日期**：2026-11-16
- **相关**：[ADR-0015](0015-toast-notifications.md)、[toast-identity-blocker.md](../development/toast-identity-blocker.md)

---

## 背景

ADR-0015 决定用 Windows 原生 Toast 代替自绘窗口（WebView2 在本机无法启动）。
实现完成后，**在这台机器上无法让 Toast 真正弹出**，且原因尚未查明。
本文记录：试过哪些 API、各自的兼容性边界、以及选型的当前状态。

## 已实现的 API：WinRT `ToastNotificationManager`

调用方式：

`ToastNotificationManager::CreateToastNotifierWithId(AUMID)`（windows crate 0.62）

### 兼容性边界（本机实测，不是文档推断）

| 调用者 | Package Identity | 结果 |
| --- | --- | --- |
| `powershell.exe`（微软签名） | 无（15700） | ✅ 成功，且记在我们的 AUMID 名下 |
| 我们的 Rust/Tauri 程序 | 无（15700） | ❌ `0x80073D54` |
| 裸 Rust 最小程序 | 无（15700） | ❌ 同上 |
| C#（同一 .NET Framework） | 无（15700） | ❌ 同上 |
| C# + 自签名并本机信任 | 无（15700） | ❌ 同上 |

**结论**：这个 API 在非打包进程**可以**工作（PowerShell 证明），
但在本机上对我们的程序不工作；差异不在 API、不在 Package Identity、不在签名有无。

## 尚未实现：Windows App SDK `AppNotificationManager`

微软当前推荐用于非打包桌面应用的通知 API。

**可行性核实结果**：

| 项 | 结论 |
| --- | --- |
| `windows` crate 是否含 AppSDK 绑定 | ❌ 只有 `Windows` 命名空间，无 `Microsoft` |
| 本地 sparse registry 是否有 AppSDK Rust 绑定 | ❌ 404 |
| Windows App Runtime 是否已安装 | ✅ 1.5 / 1.6 / 1.7 / 1.8 / 2.x |

**含义**：从 Rust 使用它需要手工声明 AppSDK 的 COM 接口与激活工厂，
不是「换个函数名」。工作量真实存在。

## 决策（当前）

1. **不切换到 AppNotificationManager**，直到完成下列判定实验——
   因为 C# 实验已经说明「换运行时/换调用路径」不是差异所在，
   换 API 未必能解决问题，而成本很高。
2. 通知呈现保持在**可替换的一层之后**（`crates/companion-core` 的 `DeliverySink`），
   换 API 不需要改数据回路。
3. 在该问题解决前，Toast 项在 STATUS 中**不得**标为 `Tested`。

## 判定实验（按顺序）

| # | 实验 | 判定 |
| --- | --- | --- |
| 1 | 把探针放在 `%ProgramFiles%` 或 System32 下运行 | 位置是否是变量 |
| 2 | 用公有 CA 证书签名后重测 | 是否要求「被认可的发布者」 |
| 3 | 临时停用 Kaspersky 后重测 | 安全软件是否参与 |
| 4 | 另一台 Windows 11（22631 / 26100） | 是否本机/版本特有 |
| 5 | C# + AppSDK `AppNotificationManager` | 换 API 是否解决 |

若 5 成功而 1–4 都失败，则选型改为 AppNotificationManager，并投入 Rust 绑定工作。

## 后果

- 通知层已实现、可编译、安装器可用，但**端到端未验证**；
- 这个问题**不阻塞其它工作**：合并规则、状态机、去重均已独立测试；
- 未来 Windows 版本升级时，请先读本文的兼容性边界表，不要重新走一遍排查。
