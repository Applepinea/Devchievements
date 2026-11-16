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

---

## 2. 历史导入（§11）

### 2.1 先修了一个**规范要求但实现缺失**的东西：统一去重

§11.2 写的是「使用统一的去重规则」。原来的实现只按 `event_id` 去重，而两条路径的事件 id 不同：

| 来源 | 事件 id |
| --- | --- |
| Hook 实时捕获 | `post-commit:<sha>:<毫秒>` |
| 历史导入 | `import:<repo>:<sha>` |

同一个提交走两条路径会**被算两次**。用户装了这个应用之后做过的提交，
一导入历史就翻倍——这是个会毁掉全部统计的错误。

修复：在摄取事务里额外按 `(repository, commit_sha, event_type)` 判重。
提交的身份是它的 sha，不是我们给它的事件 id。

### 2.2 静默导入

§11.2 要求导入期间**不生成 Outbox 通知**。加了 `NotificationPolicy::{Inform, Silent}`：

- `ingest_commit` → `Inform`（原行为不变）
- `ingest_import` → `Silent`：成就照样解锁、照样记录来源与规则版本，只是不排通知

这是**行为差异而不是开关**：静默与否是两条命名函数，调用点必须显式选择。

### 2.3 扫描为什么用 git 子进程

ADR-0002 决定 Hook 路径直读 `.git`（每次提交都跑，必须快）。
导入是每个仓库一次、要走几千个提交，**一个 git 进程吐出整份 log** 又简单又快。
这是刻意的例外，且被限制在一个模块里。

### 2.4 验证

单元/集成测试（真实 git 仓库，5 个）：

| 测试 | 断言 |
| --- | --- |
| `scanning_a_real_repository_returns_its_commits` | 读回**原始的 2023 年时间戳**，不是导入时间 |
| `importing_history_records_commits_silently` | 5 个提交入库，**Outbox 为空** |
| `importing_the_same_history_twice_is_idempotent` | 第二次 `ingested=0, duplicates=3` |
| `only_commits_by_the_configured_identity_are_imported` | 扫到 2 个、只导入 1 个 |
| `a_commit_captured_live_is_not_counted_again_by_import` | **跨路径去重**，最终只算 1 次 |

端到端 `node scripts/e2e/history-import.mjs` → **15 项全过**：

```text
  ok   it scanned three commits
  ok   all three were ingested
  ok   no notification was queued for the import
  ok   nothing new is ingested
  ok   all three are recognised as duplicates
  ok   still three events
HISTORY IMPORT PASSED: silent, idempotent, counted once
```

### 2.5 一处我改掉的自我误导

e2e 里原本有一条注释写着「验证原始提交日期」，但那一段其实只检查了计数。
真正的日期断言在集成测试里（能读到 `committed_at_ms`）。
注释已改成指向真实位置——**测试注释说的和做的不一致，比没有注释更糟**。
