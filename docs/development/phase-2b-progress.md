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
| 自定义通知弹窗（WebView2） | ✅ **已解除**（2026-11-17 更正：数据目录不可写，非沙箱禁止） |
| 系统 Toast 端到端 | ❌ 平台拒绝（见 [报告](toast-identity-blocker.md)） |
| 设置窗口 / 图鉴等全部页面 | ⬜ **已解阻塞，尚未实现** |
| shell 钩子链式执行 | ❌ 本机 `sh.exe` 无法启动 |
| 实时 IPC 通道 | ⚠️ 沙箱拒绝命名管道 |

**Phase 2b 的大部分交付物是 UI**。WebView2 已于 2026-11-17 解除阻塞（见 §4），
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

---

## 3. Outbox 可观测性（§34.1）

### 3.1 为什么不能只给一个数字

「3 条通知没送到」不是一个可行动的结论。是预约过期、是崩溃打断、还是投递一直失败？
三种情况要修的地方完全不同。

### 3.2 实现

`crates/storage/src/observability.rs`，7 个测试。

**原因码**（从状态 + `last_error` 归类）：

| 码 | 含义 |
| --- | --- |
| `interrupted_after_display_attempt` | 展示尝试已持久化后崩溃（§17.9） |
| `delivery_unknown` | 未命名的未知投递结果 |
| `delivery_failed` | 重试耗尽 |
| `suppressed_by_policy` | 按策略静默（过期/导入） |
| `all_channels_declined` | 所有通道拒绝 |
| `other` | 未识别状态**也会被统计**，不会静默消失 |

**延迟**：p50/p90/p99/max + 五档粗桶（<100ms / <1s / <10s / <1min / ≥1min）。
桶刻意粗——本地投递本来就是几十毫秒量级，细粒度只会制造虚假精度。

**`oldest_pending_ms`**：最老未完成通知的年龄。这个数字增长说明消费跟不上，
任何「每事件」指标都看不出来。

### 3.3 实测输出

```json
"outboxHealth": {
  "total": 6, "pending": 0, "oldest_pending_ms": null, "reasons": [],
  "latency": {
    "samples": 6, "p50_ms": 0, "p90_ms": 0, "p99_ms": 0, "max_ms": 0,
    "buckets": { "under_100ms": 6, "under_1s": 0, "under_10s": 0, "under_1min": 0, "over_1min": 0 }
  }
}
```

6 条全部在 100 ms 内送达（同一事务时间戳，故为 0 ms）。

### 3.4 测试

6 个纯分类测试 + 1 个**真实数据库**测试：直接插入带已知时间戳的行，
断言原因码分类与分位数计算（50/80/30000 → p50=80、max=30000）。

写这个数据库测试时连撞两个约束：`outbox.kind` 非空、
以及 `outbox.event_id` 有指向 `events` 的外键——都需要先建父行。
**这两条如果只在纯逻辑测试里验证，永远不会被发现。**

### 3.5 一处我自己写歪的断言

e2e 里我先写成「延迟必须在 under_1s 桶里」，实际数据落在 `under_100ms`，测试失败。
改成「亚秒桶合计 > 0」——断言应该描述**性质**，不是钉死某个桶。

---

## 4. WebView2 解阻塞（2026-11-17）

此前判定「内嵌 webview 无法启动」，**该判定错误**。

真正原因：WebView2 默认把用户数据放在可执行文件旁边，那个位置不可写；
API 只返回 `E_ACCESSDENIED`（`os error 5`），无法定位。

修法：`WebviewWindowBuilder::data_directory(...)` 显式指定目录。

验证：新增 `scripts/e2e/window.mjs`（4 项，含 profile 目录确实被创建的回归断言）。

**对产品的影响**：降级链的第一档（桌面浮窗）现在可用，
Toast 不再是唯一路径。

---

## 5. Settings 页面（第一个真实 UI）

### 5.1 技术栈按规范落地

§3 冻结的是 React + TypeScript，所以没有走纯 HTML 捷径：

| 项 | 值 |
| --- | --- |
| 构建 | Vite 8 |
| UI | React 19 + TS（strict，含 `noUncheckedIndexedAccess`） |
| 产物 | `dist/index.html` + 224 KB JS + 1.4 KB CSS |

Vite 的 `base: './'` 是必须的：Tauri 用 asset 协议加载页面，
绝对路径会解析到协议根，页面直接白屏。

### 5.2 页面内容

- **Identity**：邮箱增删改（§7 前置条件）
- **Privacy**：开关 + 明确说明「不加密、不检测屏幕共享」（§26.2 要求不得误导）
- **Timezone**：本地日期口径（§12）
- **Status**：事件数、仓库数、成就数、待处理通知、从未展示数、当前呈现通道、音效数量

所有数值来自 `get_status` → SQLite，**没有一处硬编码**。

### 5.3 关键设计：页面必须自报家门

窗口冒烟测试不满足于「窗口建出来了」，而是等页面调用 `ui_ready`。

```text
WINDOW created
UI READY view=settings
WINDOW OK ui=ready
```

理由很直接：**一个打开后白屏的窗口，和一个正常工作的窗口，从外面看完全一样**。
之前 WebView2 的问题能藏那么久，就是因为只验证了「进程没崩」。

### 5.4 前端依赖为什么不用 pnpm 装

pnpm 在这台机器上卡在 `Scope: all 3 workspace projects`，
而 react/vite/@vitejs 早已 hoist 在根 `node_modules` 里。

所以 `verify.mjs` 直接调 vite，并声明依赖缺失时 SKIP 而不是失败——
**CI 里 pnpm 可用时会自动走正常路径**。

---

## 6. Phase 2b 其余页面与流程

### 6.1 页面

| 页面 | 数据来源 | 空状态 |
| --- | --- | --- |
| Dashboard | statistics + activity + storage_health | 「还没有活动」 |
| Achievements | unlocked_achievements + 内置目录 | 锁定项作为目标显示 |
| Activity | daily_projection 聚合 | 「还没有记录」 |
| Repositories | repository_projection（别名） + 导入入口 | 「装钩子或导入」 |
| Settings | settings.json | 「还没有身份」引导 |
| About | capabilities + outbox + storage | 「没有投递失败」 |

**没有一处硬编码数字**，全部经 Rust 命令读 SQLite。

### 6.2 新解锁标记覆盖四种来源

§34.2 要求实时、延迟、回填、静默解锁都能显示标记。
做法是**按 `viewed_at` 判定，而不是按来源判定**：

````rust`
viewed: row.get::<Option<i64>, _>("viewed_at").is_some(),
`````

这样四种来源共用一条规则，未来新增来源也不需要改这里。

### 6.3 逐页端到端验证

问题：没法用脚本点导航。

解法：**页面 id 走 URL fragment**，冒烟测试可直达任一页。

```text
--window-smoke --page about  ->  index.html#about
UI READY view=about
```

`scripts/e2e/pages.mjs` 对 6 个页面各跑一次，断言渲染 + 自报 + 未崩溃 → **18 项全过**。

这能抓到「编译通过但运行时爆炸」的错误，构建检查抓不到。

### 6.4 卸载：残留 Hook 必须静默退出

§34.1 要求「清理 Hook 或确保残留 Hook 静默退出」。

写了 `scripts/e2e/uninstall.mjs` 之后，**它抓到一个真实缺陷**：

```text
FAIL it does not recreate the data directory
     ["spool"]
```

卸载后残留的 hook 仍然重建了数据目录——用户刚删掉的东西被复活了。

修复：helper 启动时先看 companion 的数据目录是否存在，不存在就直接退出，什么都不建。
目录由安装器和应用负责创建，**helper 只读不建**。

改完 4 项全过。这个缺陷是纯理论审查看不出来的。

### 6.5 备份与恢复

- 备份用 `VACUUM INTO`：运行中直接拷文件可能拿到撕裂的写，VACUUM INTO 由 SQLite 保证一致性。
- 恢复拒绝非 SQLite 文件，并先写到临时文件再改名——中断不会留下半个数据库。

### 6.6 一个连锁回归

helper 的新守卫让 3 个旧 e2e 失败：`hook-to-spool`、`full-loop`、`ipc-fallback`。
它们的夹具假设 helper 会自己建目录。

这不是 bug，是**行为变更**：装过的机器上目录本来就在。于是夹具改成「模拟已安装」。

顺带修了 `window.mjs` 里写死 `view=settings` 的断言（默认页已变成 dashboard）。
