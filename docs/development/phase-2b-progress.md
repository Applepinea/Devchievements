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

---

## 7. 安装向导（§25）

### 7.1 三步而不是一张长表单

```text
Welcome -> Identity -> Hooks -> Done
```

分步的理由是**每步都有前置条件**：没有身份就装钩子，会捕获到之后被拒绝的提交。

身份页会读本机 Git 全局配置（`user.email`）并给出建议，
用户点一下即可，不用手抄邮箱。

### 7.2 三个 bug，都是「看起来对」的那种

**bug 1：命令只注册到一个 builder。**

应用有两处 `generate_handler!`（托盘主窗口、冒烟测试窗口）。
我用 `String.replace` 只替换了第一处，于是冒烟窗口里命令不存在，
前端调用失败 → 走兜底 → 直接显示 dashboard。

同一个错我在这轮里犯了**第二次**（上一次是隐私模式读取）。

**bug 2：初始状态就上报了视图名。**

`onboarded === null` 时三元走 else 分支，立刻报了一次 `dashboard`。
测试看到的是「向导没出现」，而实际是「报早了」。
改为状态确定后再上报。

**bug 3：设置字段名不一致。**

Rust 字段 `onboarding_completed`，测试写的 JSON 是 `onboardingCompleted`。
其他字段都是 camelCase，这个漏了 `rename`，于是标记读不到、向导反复出现。

### 7.3 打包差异暴露成功能缺陷

hook 安装报 `devch-helper.exe is missing next to the app`。

不是逻辑错——桌面应用和 helper 构建到**两个不同目录**，
而安装器会把它们放在一起。开发构建少做了这一步，
于是「开发构建装不了钩子」。

修法：构建脚本也把 helper 复制到应用旁边，和安装器行为一致。

（顺带踩到路径层级：从 `scripts\dev` 到仓库根是两级 `..`，我写了三级。）

### 7.4 验证

`scripts/e2e/wizard.mjs` → **9 项全过**：

```text
ok  it shows the wizard instead of the dashboard
ok  a configured machine skips the wizard
ok  the wizard does not reappear
ok  the onboarding flag is persisted
ok  the wizard can install hooks
ok  all three hooks were reported
ok  the hook file exists
ok  it points at the helper
```

---

## 8. 补验收缺口（§34.2）

### 8.1 向导的导入步骤

§34.1 原文要求「历史导入界面与完整流程（**包括安装向导中的导入步骤**）」，
我原来漏了这一步——在 STATUS 里还错标成了 `Tested`，已改回。

现在向导是四步：

```text
欢迎 -> 身份 -> 钩子 -> 导入 -> 完成
```

为了让每一步都能被验证，**步骤也能通过 URL 直达**：

```text
--window-smoke --page welcome --step import   ->  index.html#welcome?step=import
UI READY view=welcome-import
```

`wizard.mjs` 因此能逐步骤断言，从 9 项扩到 **13 项**。

### 8.2 备份与恢复：从「写了」到「验过」

之前只有实现没有测试，我把它从 `Tested` 降回 `Partially Implemented` 是对的。

新增 `backup-restore.mjs`，**13 项**，做的是真操作：

```text
导入 3 个提交 -> 备份 -> 删掉数据库 -> 恢复 -> 对比事件/仓库/成就
再拿一个不是数据库的文件去恢复 -> 必须被拒且数据无损
```

只断言「备份文件存在」是不够的——那只能证明写了个文件。

### 8.3 升级路径

新增 `upgrade.mjs`，**13 项**，覆盖版本变更真正会伤到的三处：

| 位置 | 断言 |
| --- | --- |
| 旧配置文件 | 含未知字段仍能读取，邮箱不丢 |
| 已装钩子 | 重复安装成功且文件不被破坏（两次结果一致） |
| 既有数据库 | 重新打开后计数不变、schema 版本可读 |

### 8.4 两个「测试写错了」而不是「产品坏了」

1. `--onboarding` 返回的是 Git 建议邮箱，不是已配置邮箱。
   断言看错字段 → 改成「重新保存后邮箱仍在」，那才证明配置文件被正确读入。
2. 升级测试用的仓库**没有任何提交**，`git log HEAD` 直接失败。
   补了一个提交——这不是产品问题，是夹具没准备。

---

## 9. 补齐 Settings 的四项（§34.1）

### 9.1 补了什么

| 项 | 实现 |
| --- | --- |
| 通知开关 | 关掉后不弹窗，**但成就照样记录** |
| 音量与静音 | 0–100 滑杆 + 静音；只影响本应用 |
| 运行模式 | 显示 Desktop Only，并说明扩展能带来什么 |
| 数据管理 | 打开目录、立即备份、清除统计（两步确认） |

### 9.2 音量为什么不是调系统音量

`PlaySoundW` 没有单次音量参数。现成的 `waveOutSetVolume` 调的是**整个声卡**音量——
用户想关掉成就音，不该把他正在听的音乐也关小。

所以按采样点缩放 16-bit PCM，再用 `SND_MEMORY` 播放：
只影响我们自己的声音。非 16-bit 的 WAV 原样播放——
用错增益播出来比保持原样更糟。

### 9.3 通知开关的语义

关掉通知**不影响记录**。这不是妥协，是规范要求的分工：
开关控制「打断」，不控制「算不算数」。
e2e 里专门有这一条断言，因为写错的开关会让成就悄悄停止累计。

### 9.4 又抓到一次空比较

备份测试里我写的是 `snapshot.unlockedAchievements`，
而诊断快照实际输出 `unlocked_achievements`（snake_case）。

于是那条断言变成了 **undefined === undefined**，永远通过。

这不是「测试没跑」，是**测试跑了但什么也没验**——比没有测试更危险。
已在备份测试里补了一条前置断言（快照必须真的有成就），
让空比较无法再发生。
