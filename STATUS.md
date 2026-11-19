# STATUS — Devchievements

> **唯一权威进度表。**
>
> 状态取值严格限定为：
> `Not Started` / `In Progress` / `Implemented` / `Tested` / `Partially Implemented` / `Platform Unverified` / `Blocked`
>
> **不可混淆的五件事**（AGENTS.md §10）：代码已写 / 构建已过 / 自动化测试已过 / 实际运行测试已过 / 平台兼容性已验证。
>
> 最后更新：2026-11-19 —— **Phase 4b 进行中**（安装包/商店/扩展链路已打通；本轮修 14 项审查待办，见下方「已知功能缺口」与本轮变更记录）

---

## 唯一验证命令

```powershell
node scripts/verify.mjs
```

当前结果（真实执行，非推测）：

```text
PASS  fmt                       291 ms
PASS  clippy                   2426 ms
PASS  test                    16996 ms   502 tests passed, 0 failed
PASS  build                    3091 ms
PASS  build-wasm               2697 ms
PASS  desktop-ui-typecheck     1503 ms
PASS  desktop-ui-build          444 ms
PASS  desktop-build           17531 ms
PASS  extension-build          1239 ms
PASS  extension-test            204 ms
ALL CHECKS PASSED
```

最后实测：**2026-11-20 01:30**（`.verify/summary.json` 有机器可读记录）。改这行数字的唯一方法
是重跑 `node scripts/verify.mjs`。

桌面 crate 的单元测试不在 `verify.mjs` 里（它是独立 cargo 工程），单独跑：
`cmd /c "scripts\dev\build-desktop.bat test --bins"` → **51 passed**（audio 14、settings 8、
theme_store 8、preflight 7、update 6、autostart 4、local_helper 4）。

桌面应用是**独立**的 cargo 工程（不能用 workspace 的 GNU 工具链；用 workspace 的 cargo 会
报 `dlltool.exe: program not found`），所以它的测试不在 `verify.mjs` 里。它自己的构建步骤
`desktop-build` 在 `verify.mjs` 里，但**只构建不测试**，单测要单独跑（上一段）。

原始日志与机器可读摘要：`.verify/`

### 本轮新增/修改的 e2e 脚本（全部实测 PASS）

| 脚本 | 检查项 | 覆盖 |
| --- | --- | --- |
| `scripts/e2e/theme-manager.mjs` | 30 | 内置主题存在/激活、导入、应用改 `settings.json`、重启后保留、含 `script` 包被拒、含 `css` 包被拒、越界值被夹、导出可回导、内置不可删、删用户主题、恢复默认、**恢复默认不删用户主题包** |
| `scripts/e2e/builder-form.mjs` | 17 | 表单线格式被接受、写进 settings、rule version、导出 round-trip、**真实 hook 记录 + 真实 drain 后解锁**、超深被拒、条件过多被拒、shadows built-in 被拒、重复 id 被拒、拒绝后不落盘 |
| `scripts/e2e/csp.mjs` | 8 | 启动无 CSP 违规、bundle 渲染、注入的 inline `<script>` 不执行、渲染进程报告 `script-src` 违规、inline 事件处理器不执行、inline style 仍生效、应用仍响应、native 侧无 renderer 错误 |
| `scripts/e2e/release-signing.mjs` | 15 | 开发路径用仓库密钥并警告；`DEVCH_RELEASE=1` 缺 key → 退出码 7 且不回落；缺口令 → 退出码 7；两者齐全 → **CI 提供的 key/口令不被覆盖**；脚本自身文档化两条路径 |
| `scripts/e2e/sound-preview.mjs` | 24 | 试听走**真实选择路径**：内置文件、用户覆盖、每日等级槽位都播「按当前设置真正会响的那一个」；静音与音量 0 如实报告「选了哪个、为什么没播」而不是假装播放；用户文件被删后回落内置而不是变哑；悬空覆盖仍能回答并说明它想要哪个 |
| `scripts/e2e/cross-repository.mjs` | 16 | 三个仓库的提交合起来算每日等级与累计成就；仓库页仍显示各自计数；六个提交产生六条独立可重复触发与六条独立通知；触发记录不进入永久表、不进入每日里程碑 |
| `scripts/e2e/sound-serialized.mjs` | 7 | 一次投递连续两条通知时，第二个声音必须等第一个放完（有等待 865 ms / 去掉等待 22 ms，而第一个声音本身 778 ms）；两个弹窗都出现；`PlaySoundW` 没有拒收缓冲区 |

`scripts/e2e/` 现有 **46** 个脚本；本轮新增 7 个（`theme-manager` / `builder-form` / `csp` /
`release-signing` / `sound-preview` / `cross-repository` / `sound-serialized`），修改 6 个
（`browser-achievement` +4 项、`ui-pages` 加 builder 页、`native-host` / `browser-to-popup` /
`login-mapping` 随 origin 校验调整、`preview-button` 修掉对自己诊断行的误判）。

### T14 测试名审计（quiet / silent / no / never）

规则（已写入 [AGENTS.md](AGENTS.md) §10）：名字声称「无声」或「无副作用」的测试，**断言对象必须是副作用集合本身**。

审查范围：`crates/**` 里所有名字含 `quiet` / `silent` / `no_` / `never` / `not_` 的测试。

**已符合**（断言的是副作用集合）：

| 测试 | 断言对象 |
| --- | --- |
| `empty_commits_produce_no_output` | `EngineOutput::is_empty()`（同时覆盖 `state_changes` 与 `side_effect_intents`） |
| `facts_that_do_not_count_produce_no_changes` | `EngineOutput` 的两个集合 |
| `a_second_merged_pull_request_counts_but_does_not_announce_again` | `out.side_effect_intents.is_empty()` |
| `already_unlocked_achievements_are_not_re_announced` | `side_effect_intents` 里没有对应项 |
| `importing_history_records_commits_silently` | `outbox_counts()` 为空 |
| `a_quiet_policy_records_the_unlock_without_queueing_a_notification` | `SELECT … FROM outbox` 为空 |
| `a_plain_commit_produces_a_sound_and_no_popup` | `Present**ation` 变体（纯函数，无副作用集合，名字描述的是返回值） |
| `an_old_record_still_counts_but_is_quiet` | **本轮补齐**：`report.queued_notifications == 0` + `pending_notifications()` 为空（原先只看 `ingested`） |
| `a_weekend_replay_produces_no_windows` | **本轮新增**：`sink.shown` 里没有 `Popup`，且 `shown <= 1` |
| `a_digested_commit_with_no_reward_stays_completely_silent` | **本轮新增**：`server.shown.is_empty()` + outbox 无行 |
| `a_plain_commit_plays_a_sound_and_shows_no_popup` | `sink.shown[0] == Presentation::SoundOnly`（列表本身） |
| `not_a_repository_is_reported_and_never_a_panic` | 断言的是 `Err` 而非 panic（无副作用集合可断言） |
| `an_unavailable_channel_never_blocks_the_hook` | 断言返回类型；该层没有副作用集合 |
| `a_nonexistent_local_time_does_not_panic` | 同上，断言不 panic |

**没有需要改名或补断言的项。** 三处「名字声称安静但断言不足」的地方（上面标**本轮**的三行）已在本轮修复。

### 「Tested」抽查：命令能复现

T10 的验收要求是「随机抽 10 个 Tested 项，命令都能复现」。2026-11-19 抽查结果：

| 状态项 | 复现命令 | 结果 |
| --- | --- | --- |
| 1.21 Builder 复杂度 | `node scripts/cargo-run.mjs test -p devchievements-achievements` | exit 0 |
| 1.16 Golden Tests | `node scripts/cargo-run.mjs test -p devchievements-golden-tests` | exit 0 |
| 2a.14 通知队列与合并 | `node scripts/cargo-run.mjs test -p devchievements-notifications` | exit 0（27 项） |
| 2a.20 新鲜度三档 | `node scripts/cargo-run.mjs test -p devchievements-companion-core` | exit 0（20 项） |
| 3b.8 浏览器活动成就 | `node scripts/e2e/browser-achievement.mjs` | PASSED |
| 4a.1 资源安全 | `node scripts/cargo-run.mjs test -p devchievements-assets` | exit 0 |
| 4a.3 主题包 + 界面 | `node scripts/e2e/theme-manager.mjs` | PASSED（30 项） |
| 4a.8 Achievement Builder | `node scripts/e2e/builder-form.mjs` | PASSED（17 项） |
| 4b.6 多语言 | `node scripts/check-i18n.mjs` | exit 0（225/226，1 条未用） |
| 4b.11 向导/页面可达性 | `node scripts/e2e/ui-pages.mjs` | PASSED（7 页 × 2） |

另外本轮新增：`node scripts/e2e/csp.mjs` → PASSED（严格 CSP 生效，注入的内联脚本被拒）；
`node scripts/e2e/sound-serialized.mjs` → PASSED（7 项；把令牌 `Drop` 里的等待去掉后**同一脚本失败**，第二个声音只晚 22 ms 就开播）。

---

## 当前阶段

| 阶段 | 状态 | 备注 |
| --- | --- | --- |
| Phase 0 项目基础与技术验证 | `Partially Implemented` | 归档；GitHub 仓库仍未创建 |
| Phase 1 Core Engine 与数据层 | `Tested` | 完成 |
| Phase 2a 本地 Git 到通知闭环 | `Tested` | Toast 通道已由自有弹窗取代（ADR-0015） |
| Phase 2b Desktop Only 产品 | `Tested` | 仅 Windows 实测（ADR-0017） |
| Phase 3a Browser Only | `Partially Implemented` | Outbox 队列与页面未做，详见 3a 表 |
| Phase 3b Full Experience | `Tested` | 扩展检测 + 桌面判定（ADR-0018） |
| Phase 4a 自定义系统 | `Tested` | 主题管理与 Achievement Builder 本轮补齐 |
| Phase 4b 产品化与发布准备 | `Partially Implemented` | 见 4b 表；平台签名与发布流水线仍受阻 |

**检查点 A / B / C 均已获用户明确批准**（B：「继续 phase3」；C：「进入 phase4」；4b：「继续做phase4b」）。
当前**没有待批准的检查点**。

---

## Phase 3b 任务表（Full Experience：扩展检测 + 桌面判定）

> 2026-11-17 产品决定：扩展只负责**检测**，判定与弹窗由本地桌面应用完成。
> 这条改变了 §36（Browser Only）的边界，见 ADR-0018。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 3b.1 | Native Messaging 协议（4 字节长度 + JSON） | `Tested` | `native-host` e2e 11 项：真实管道、真实分帧 |
| 3b.2 | 浏览器报告 → 本地判定 → 入库 | `Tested` | 同一 e2e：companion drain 后诊断显示 1 个提交 |
| 3b.3 | 扩展侧转发 + 本地留痕 | `Tested` | `extension-load` 18 项：真实 `connectNative` 握手 + 本地存储 |
| 3b.4 | Host 清单与浏览器注册（固定扩展 ID） | `Tested` | `register-native-host.mjs` 写清单 + HKCU 注册；ID 由提交的公钥推导并与加载一致 |
| 3b.5 | 桌面弹窗呈现浏览器事件 | `Tested` | `browser-to-popup` e2e 6 项：浏览器字节 → host → 桌面弹窗 |
| 3b.6 | 注册与清理脚本 | `Tested` | `register-native-host.mjs` / `unregister-native-host.mjs`，只动 HKCU |
| 3b.7 | PR / Issue 识别 | `Tested` | 扩展 14 项测试；Rust 分类 opened/merged/closed；host 转发标志位 |
| 3b.8 | 浏览器活动 → 计数 / 成就 / 弹窗 | `Tested` | `browser-achievement` e2e 22 项（真 binary、真 DB）：PR Merge 计数并解锁 **Merged**、PR Opened 只入库、重复报告只算一次、Release 解锁 **First Release**；4 个 Rust 测试锁住「PR 不算提交」「不移动每日等级」；ADR-0019 |

---

## Phase 3a 任务表（Browser Only）

> 检查点 B 已由用户以「继续 phase3」明确批准。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 3a.1 | 扩展可独立安装并在真实浏览器运行 | `Tested` | `extension-load` e2e 7 项；CDP `Extensions.loadUnpacked` |
| 3a.2 | Rust 核心在 MV3 worker 内实例化（CSP） | `Tested` | 同一脚本断言 WASM 实例化与探针调用 |
| 3a.3 | GitHub 活动识别（Commit/Release） | `Tested` | 9 个单元测试（URL/日期/去重键）+ `extension-load` 端到端入库 |
| 3a.4 | IndexedDB 持久化 + 独立 Outbox | `Partially Implemented` | IndexedDB 存储与去重已端到端验证；**Outbox 队列尚未实现** |
| 3a.5 | 浏览器通知与音效 | `Not Started` | |
| 3a.6 | 浏览器端设置/Dashboard/图鉴 | `Partially Implemented` | Dashboard 读 IndexedDB 并渲染（端到端 3 项）；**设置与图鉴未做** |
| 3a.7 | 仓库身份映射（remote → canonical） | `Tested` | 钩子记录 remote；hook 与 browser 归并为同一仓库；无 remote 保持 local |
| 3a.9 | 用户身份映射（GitHub Login → Email） | `Tested` | 设置界面可编辑；`login-mapping` e2e 7 项（未映射拒绝 → 映射后计数） |
| 3a.8 | Service Worker 回收后状态恢复 | `Tested` | `service-worker-restart` e2e：真实停掉 worker 后数据仍在 |

---

## Phase 2b 任务表

> 用户已明确指示进入 Phase 2b。跨检查点时的未验证项见
> [phase-2b-progress.md](docs/development/phase-2b-progress.md) 第 0 节。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 2b.1 | 平台能力探测与降级链（§29） | `Tested` | 13 个测试；运行时实测降级到 in-app-only |
| 2b.2 | 兼容性矩阵（§29.6） | `Tested` | Windows 行实测；macOS/Linux 标为「不支持（ADR-0017）」 |
| 2b.3 | Dashboard / Gallery / Activity / Repository Manager / About | `Tested` | 5 个页面全部读真实数据库；`pages` e2e 18 项通过 |
| 2b.4 | 历史导入界面与完整流程 | `Tested` | 管线 + Repository Manager + **向导导入步骤**；静默/幂等/跨路径去重/原始日期已测 |
| 2b.5 | 基础 Settings | `Tested` | 身份/通知开关/音量与静音/隐私/运行模式/时区/数据管理（打开目录、备份、清除）；`notification-settings` e2e 12 项 |
| 2b.6 | 安装向导 / 卸载机制 | `Tested` | 三步向导（身份→钩子→完成）+ 残留 Hook 静默退出；`wizard` e2e 9 项、`uninstall` 4 项 |
| 2b.7 | 数据备份与恢复 | `Tested` | `backup-restore` e2e 13 项：真备份、真删除、真恢复、数据一致、拒收非数据库 |
| 2b.8 | Outbox 可观测性（延迟分布与原因码） | `Tested` | 7 个测试（含真实数据库）；已并入 `--diagnostics` |
| 2b.9 | 空状态与引导（§34.1 补充） | `Tested` | 每页有空状态；无身份时给出引导；卸载后目录不被复活 |
| 2b.10 | 通知弹窗窗口（§28） | `Tested` | 不抢焦点/不进任务栏/置顶/自动关闭；数据随片段传入 |
| 2b.11 | 升级路径（§34.2） | `Tested` | `upgrade` e2e 13 项：旧配置文件、钩子重复安装、数据库重开 |

---
## Phase 2a 任务表

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 2a.1 | Rust Helper（post-commit / post-rewrite / post-merge） | `Tested` | 真实 `git commit` 触发，端到端通过 |
| 2a.2 | Hook 安装 / 备份 / 链式 / 卸载 | `Tested` | 12 个测试；**shell 链式未真实验证**（见 §32 文档） |
| 2a.3 | IPC 帧协议 | `Tested` | 9 个测试，含超限长度与截断 |
| 2a.4 | Spool 回退 | `Tested` | 11 个测试，含并发与损坏恢复 |
| 2a.5 | 实时 IPC 通道（客户端 + 服务端） | `Platform Unverified` | 已实现并编译；**本机沙箱拒绝命名管道**（os error 5）。回退路径已验证 |
| 2a.6 | Tauri Companion 与托盘 | `Tested` | 编译/链接/托盘运行/真实提交驱动/窗口创建均已验证；托盘含隐私开关、打开窗口、手动 drain |
| 2a.7 | SQLite 持久化接入 Helper | `Tested` | 完整闭环：commit→hook→helper→spool→companion→SQLite |
| 2a.8 | Outbox Consumer（Desktop 子集） | `Tested` | 投递、重试、`delivery_unknown` 均有测试 |
| 2a.9 | 桌面弹窗与提示音 | `Partially Implemented` | **提示音已完成并实测**；Toast 注册与调用已实现（安装器 exit 0），**端到端未通过** |
| 2a.15 | 通知呈现 | `Tested` | **改用自有弹窗**（ADR-0015）：真实提交触发真实窗口，`popup` e2e 8 项通过；系统通知通道已移除 |
| 2a.16 | AppNotificationManager 替代路线 | `Not Started` | 见 [ADR-0016](docs/decisions/0016-notification-api-choice.md) |
| 2a.17 | Sparse Packaging（若确需 Package Identity） | `Not Started` | 见 ADR-0016 |
| 2a.10 | Companion 启动消费 Spool | `Tested` | 无头 Companion 真实消费并计数 |
| 2a.11 | 最小诊断视图 | `Partially Implemented` | **数据侧完成**：`--diagnostics` 输出真实 JSON，仓库别名化、路径脱敏，12 项 e2e 校验；UI 需 WebView2 |
| 2a.14 | 通知队列与合并（§16.4、§16.6） | `Tested` | 一个事件一个弹窗；普通提交只出声不弹窗 |
| 2a.20 | 新鲜度三档（§16.3） | `Tested` | 0–10 分钟弹窗、10 分钟–24 小时**一条汇总不弹窗**、超过 24 小时静默；`the_freshness_bands_decide_what_the_user_sees`（9m / 11m / 23h59m / 24h01m）+ `a_weekend_replay_produces_no_windows`（50 条重放 0 弹窗 ≤1 汇总）+ `browser-achievement` e2e 中间档 4 项。**未做**：汇总形态是托盘 tooltip + 日志，无独立窗口；文案未本地化；阈值不可调 |
| 2a.18 | 提示音：内置默认 + 基础播放（§19） | `Tested` | 15 个选择逻辑测试；真实提交实测；奖励音为用户提供的 achievement.wav。**2026-11-19 补修播放本身**：缓冲区跨播放存活、两个声音不重叠（`sound-serialized` e2e 7 项，正反两跑）|
| 2a.13 | Privacy Mode 基础行为（§26.2） | `Tested` | 26 个测试；**托盘开关 + 持久化 + 通知文案实测**（8 项 e2e） |
| 2a.19 | 设置文件（§26.4 版本化配置） | `Tested` | `settings.json`：身份邮箱、隐私模式；未知字段忽略 |
| 2a.12 | Crash Recovery 矩阵（§32.5） | `Partially Implemented` | 6 个场景中 4 个已真实通过；2 个需实时 IPC |

> 详见 [phase-2a-progress.md](docs/development/phase-2a-progress.md)。
> 端到端：`node scripts/e2e/hook-to-spool.mjs`、`node scripts/e2e/full-loop.mjs`、
> `node scripts/e2e/ipc-fallback.mjs`、`node scripts/e2e/crash-recovery.mjs`

---

## Phase 1 任务表（已完成）

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1.1 | 标准事件模型与验证 | `Tested` | `crates/events`，31 个测试 |
| 1.2 | 来源可信度 | `Tested` | 含「unverified 不计入官方统计」断言 |
| 1.3 | 仓库身份识别 | `Tested` | SSH/HTTPS/git 后缀/大小写/跨平台不合并 |
| 1.4 | Commit 去重键 | `Tested` | `repo_id + sha`；patch id 仅辅助 |
| 1.5 | 用户身份识别（多邮箱） | `Tested` | 身份模型 + 优先级 + GitHub login 映射 + Git 配置读取；**未配置时禁止统计已强制** |
| 1.6 | 时区系统 + 日界线 | `Tested` | 内置 TZDB；DST 与 04:00 边界已测 |
| 1.7 | 每日等级与每日里程碑 | `Tested` | 14 个测试；用户阈值不影响官方口径；**等级看当天跨仓库合计**（ADR-0021），`cross-repository` e2e 覆盖 3/2/1 → 6 次 → Rare |
| 1.8 | 永久成就（含 required_capabilities） | `Tested` | 13 个测试；能力由条件推导 |
| 1.9 | 用户过滤器 | `Tested` | 18 个测试；官方口径与用户口径在类型上分离 |
| 1.10 | 统计投影 | `Tested` | 读写 + 可重建性均已测（`rebuild_from_events` 复现同值，**含全局每日表**）；`a_rebuild_produces_the_same_global_numbers_as_live_ingestion` 守护 |
| 1.11 | Engine（含读集） | `Tested` | 7 个测试；读集有断言守护 |
| 1.12 | SQLite Storage Adapter 与 SQLx 迁移 | `Tested` | 版本化 schema（当前 **v5**）+ 真实库测试；**迁移前自动备份**（`VACUUM INTO` 到 `backups/`，备份失败即中止迁移），v3 → v4 回填由 `upgrading_an_old_database_backs_up_first_and_keeps_every_unlock` 覆盖，v4 → v5 加表由 `upgrading_v4_to_v5_keeps_the_permanent_unlocks_and_adds_the_trigger_table` 覆盖（重复迁移不再备份、不重建） |
| 1.13 | 事件持久化 | `Tested` | 事件+投影+解锁+Outbox **同一事务**，有测试守护 |
| 1.14 | Outbox 与通知所有权状态机（纯逻辑） | `Tested` | 18 个测试；合法转换表 + 防重复展示 |
| 1.15 | 历史导入管线（引擎侧） | `Tested` | 用提交时间归档、幂等、可续传；跨日用例已测 |
| 1.16 | Golden Tests | `Tested` | 16 个测试回放 15 份真机 trace |
| 1.17 | 属性测试 | `Tested` | 11 个属性（时区 5 / Hook 顺序 6），各上千用例 |
| 1.18 | packfile 解析 | `Tested` | **本轮解除风险 A3**：真实 gc 仓库上 10 个历史提交可读。见 [ADR-0013](docs/decisions/0013-packfile-support.md) |
| 1.19 | IndexedDB Adapter 接口设计 | `Implemented` | 只有 trait 与对象存储名常量（§31.1 要求「不展开」） |
| 1.20 | 初步性能基准 | `Tested` | 摄取 p95 **3.12 ms**，n=2000，release |
| 1.21 | Achievement Builder 复杂度与确定性限制 | `Tested` | 深度 5 / 条件 50 / **步数预算**（非墙钟）；13 个测试 |

> 详见 [phase-1-progress.md](docs/development/phase-1-progress.md)。

---

## 当前阶段（Phase 0 归档）

**Phase 0：项目基础与技术验证** —— 状态 `Partially Implemented`

> 归档节。三个遗留项里，MSVC 工具链（ADR-0014）与浏览器启动已在后续 Phase 解决；
> 只有「GitHub 仓库」仍未创建。

---

## Phase 0 任务表

| # | 任务 | 状态 | 证据 / 备注 |
| --- | --- | --- | --- |
| 0.1 | 检查目录、Git 状态、工具链 | `Tested` | [environment-baseline.md](docs/development/environment-baseline.md) |
| 0.2 | Monorepo 初始化 | `Tested` | `cargo build --workspace` 通过 |
| 0.3 | 根 AGENTS.md | `Implemented` | [AGENTS.md](AGENTS.md) |
| 0.4 | 根 STATUS.md | `Implemented` | 本文件 |
| 0.5 | 规范拆分 docs/spec/ | `Tested` | 49 章 → 47 文件，覆盖率断言通过 |
| 0.6 | ADR 框架与 docs/development/ | `Implemented` | 7 份有效 ADR + 模板 + 索引 |
| 0.7 | 基础 CI | `Tested` | **2026-11-20 首次在 GitHub Actions 上跑绿**（run 44568079398：Rust fmt/clippy/test/build、Node 扩展链路、多 Git 版本矩阵三个 job 全部 success）。首跑三个 Git 矩阵 job 失败，根因是 PortableGit 资产名少一个连字符（`64bit` → `64-bit`），GitHub 404 返回 HTML 被当作自解压包保存；已修并加 10 MB 大小守卫（c3c2b30）。Pages 源由 branch/README 切到 GitHub Actions，隐私政策 URL 已实测 200 |
| 0.8 | Tauri 2 启动 + React + TS | `Tested` | **已解除**（ADR-0014 引入 MSVC 工具链）：`scripts/dev/build-desktop.bat build` 通过，`verify.mjs` 的 `desktop-build` 每次执行；真实窗口由 `ui-pages` / `popup` / `csp` e2e 驱动 |
| 0.9 | WXT + MV3 构建 | `Tested` | WXT 0.20.27，`verify.mjs` 每次都会重建 |
| 0.10 | Native Messaging 最小通信 | `Implemented` | Rust 侧 framing 已实现并有单测；**浏览器联调未做**（沙箱） |
| 0.11 | **SQLite + SQLx 读写与迁移** | `Tested` | **本轮解除阻塞**：5 个真实数据库测试全通过（往返、幂等、事务回滚、级联、重开保留） |
| 0.12 | Rust Core 编译为 WASM | `Tested` | 27,569 bytes；实例化 0.606 ms；与 native 逐字节一致 |
| 0.13 | MV3 CSP `wasm-unsafe-eval` 真实浏览器验证 | `Platform Unverified` | manifest 含该 CSP 且 wasm 已打包；**浏览器无法启动** |
| 0.14 | Helper 实现方案对比 | `Tested` | **本轮补齐方案 B**：三方案全部实测。见 [ADR-0002](docs/decisions/0002-hook-helper-implementation.md) |
| 0.15 | Git 行为实验（15 场景） | `Tested` | [git-behavior-experiments.md](docs/development/git-behavior-experiments.md) |
| 0.16 | 最低 Git 版本结论 | `Implemented` | 三版本实测（2.30.2 / 2.40.1 / 2.54.0 各 15/15 场景，2.20.1 直接失败）→ **最低 2.30.x**，ADR-0003 改 `Accepted`。**未验证**：2.20.1–2.30.2 之间无数据；矩阵在 GitHub Actions 上的首跑（2026-11-20）三版本全部 success。见 [ADR-0003](docs/decisions/0003-minimum-git-version.md) 与 [git-version-matrix.md](docs/development/git-version-matrix.md) |
| 0.17 | 分类等待窗口 | `Tested` | 实测 436–452 ms → 改 800 ms。见 [ADR-0004](docs/decisions/0004-rewrite-classification-window.md) |
| 0.18 | trace fixtures + JSON Schema | `Tested` | 15 份 fixture，Schema 与 Rust 侧双重校验 |
| 0.19 | 桌面上通知能力实测 | `Not Started` | Phase 0 无 Companion，无法验证 |
| 0.20 | `external-accounts-and-secrets.md` | `Implemented` | [链接](docs/development/external-accounts-and-secrets.md) |
| 0.21 | 时区策略结论 | `Implemented` | [ADR-0005](docs/decisions/0005-timezone-strategy.md) |
| 0.22 | **风险分析（§48.3）** | `Implemented` | **本轮补写**：[risk-analysis.md](docs/development/risk-analysis.md) |
| 0.23 | **Phase 0 实施计划（§48.5）** | `Implemented` | **本轮补写**：[phase-0-plan.md](docs/development/phase-0-plan.md) |

---

## Phase 0 强制 ADR 结论（规范 §30.5）

| ADR | 主题 | 状态 | 证据 |
| --- | --- | --- | --- |
| [0001](docs/decisions/0001-wasm-strategy.md) | WASM 方案 | `Partially Accepted` | 4/6 验证通过；浏览器内加载未验证 |
| [0002](docs/decisions/0002-hook-helper-implementation.md) | Hook 实现方案 | `Accepted` | ✅ **三方案全部实测**（A 38.7 / B 66.2 / C 13.4 ms p95） |
| [0003](docs/decisions/0003-minimum-git-version.md) | 最低 Git 版本 | `Proposed` | ❌ 单一版本，证据不足 |
| [0004](docs/decisions/0004-rewrite-classification-window.md) | 分类等待窗口 | `Accepted`（数值 Provisional） | ✅ 三次实测一致 |
| [0005](docs/decisions/0005-timezone-strategy.md) | 时区策略 | `Accepted`（选型） | 选型有据；实现待 Phase 1 |

**诚实说明**：规范要求 5 项全部有明确结论。实际达成 3 项 Accepted、1 项 Partially Accepted、
1 项因**只有一个 Git 版本的数据**保持 Proposed。编造最低版本会直接违反 §44.2。

---

## 实测数字汇总

| 指标 | 实测值 | 目标 | 判定 |
| --- | --- | --- | --- |
| Hook 耗时（直读 .git，p95） | **13.40 ms** | < 50 ms | ✅ |
| Hook 耗时（gix，p95） | 66.16 ms | < 50 ms | ❌ 超预算 |
| Hook 耗时（单次 git，p95） | 38.73 ms | < 50 ms | 🟡 吃掉 77% 预算 |
| Hook 耗时（朴素外派，p95） | 428.26 ms | < 200 ms 硬超时 | ❌ 违反 |
| 自动化测试 | **42 个通过 / 0 失败** | — | ✅ |
| `cargo clippy -D warnings` | 0 警告 | 0 | ✅ |
| WASM 产物 | 27,569 bytes | < 2 MB | ✅ |
| WASM 实例化 | 0.606 ms | — | ✅ |
| native ↔ wasm 输出一致性 | 逐字节相同 | 相同 | ✅ |
| SQLite 真实往返测试 | 5 个通过 | — | ✅ |
| **Hook 耗时（出货 helper，p95，n=200）** | **10.81 ms**（p50 9.35） | < 50 ms | ✅ 2026-11-17 实测 |
| **Commit → 通知 p95（n=200，声音起播）** | **274 / 328 ms**（两次独立运行，p50 ≈ 261） | < 500 ms | ✅ 修掉 500 ms 轮询后 |
| Commit → 弹窗首帧（n=1） | 387–424 ms | < 500 ms | 🟡 样本太少，不作结论 |
| **空闲 CPU（仅托盘，45 s）** | **0.12% 整机 / 2.40% 单核** | < 1% | ✅ 整机口径；**开窗口未测** |
| 修轮询前的 Commit → 通知 p95 | 1004 ms（p50 737） | < 500 ms | ❌ 已修 |

> 每个数字都能在 `.verify/` 或 `scripts/**/.out/*.json` 找到原始样本。未实测的格子明确标注「未测量」。

> 2026-11-18 性能跟进调整了 spool 计数的内存用法、托盘空闲扫描周期、历史导入分配和扩展 IndexedDB 查询。
> 托盘扫描周期改动已**实测并回退**：100 ms 下 commit→通知 p95 升到 437 ms（预算的 87%），
> 50 ms 下为 277 ms，空闲代价 0.195% 整机。取舍与数据见 `docs/development/performance-optimizations.md`。

---

## 平台兼容性

见 [platform-matrix.md](docs/development/platform-matrix.md)。

- 可测：Windows 11（Hook 行为已实测通过）
- **不可测**：macOS、Linux、Brave；浏览器（沙箱禁止启动）

---

## 阻塞项

| 阻塞 | 影响 | 需要什么 |
| --- | --- | --- |
| 无 Authenticode 证书 | 安装包无代码签名，SmartScreen 会警告（更新签名 `.sig` 是另一回事，不能替代） | 购买/提供证书 |
| 无 GitHub 仓库 | CI 与多 Git 版本矩阵**从未在 GitHub Actions 上运行**；本地已用三个 PortableGit 版本实测 | 用户创建仓库并授权 |
| 沙箱禁止 `sh.exe` | shell 钩子链式执行、`git submodule add` | 中立环境复测 |
| `cli` 命令模式无法写 HKCU | 开机自启动、native host 注册表项的真机行为 | 真机/提权环境验证 |

> 已解决（历史）：MSVC 工具链（ADR-0014）、浏览器启动（`extension-load` / `native-host` e2e 已真实跑通）。

---

## 已知问题（产品层面，未修）

1. ~~**自定义成就永远不会解锁。**~~ **已修复（2026-11-18）。** 原来的原因是：设置页把定义
   写进 `settings.json` 的 `userAchievements`，但没有任何代码把它交给评估路径。
   现在的链路是 `settings → CompanionConfig.user_rules → ingest_commit_with_rules`，
   在同一事务内用**官方统计**判定，并复用内置成就的记录路径（解锁行、outbox、
   导入时静默），所以两者不会出现语义分叉。GUI 在每次 drain 前重读规则，
   删除的规则下一次提交就失效。回归由 `scripts/e2e/user-achievements.mjs` 守住：
   真实 git 提交 → hook → helper → drain → 解锁。
   **仍缺的**：Builder 的表单界面（目前只能写 JSON），所以状态仍是 `Partially Implemented`。
1b. **渲染进程的异常现在会进日志。** 之前 UI 崩了只会得到空白窗口：webview 的控制台
   用户打不开，Rust 侧只知道「没有任何视图报告就绪」。已加 `ui_error` 命令与全局
   `error` / `unhandledrejection` 处理，异常会以 `UI ERROR ...` 出现在 companion.log。
   **这是被一次真实事故逼出来的**：向导里把 `useState` 写在依赖它的 `useEffect` 之后，
   压缩后变成 TDZ（`Cannot access 'y' before initialization`），窗口全白。
2. **成就名与等级名尚未本地化。** 界面已中英双语（4b.6），但内置成就目录
   （`First Commit`、`Common Day`…）和隐私模式通用文案来自 Rust 侧，仍是英文。
   **等成就列表定稿后再做**：现在就翻会在目录变动时白改一轮（用户 2026-11-17 决定）。
   补齐方式是让 Rust 也读语言设置、在生成文案时选语言。
3. **一个坏字段会重置整份设置。** `settings.json` 解析失败时整体回退到默认值，
   所以手改文件时把 `popup` 写成 `{"displaySeconds":1}`（缺必填字段）会连 `emails` 一起丢，
   应用随后退化成「从 git config 猜身份」。同一现象在做性能夹具时踩到。

---

## 已知问题（环境层面，非产品缺陷）

1. **schannel TLS 损坏** → git 用 `http.sslBackend=openssl`；cargo 用本地 sparse 代理。
2. **`.ps1` 必须纯 ASCII**（PowerShell 5.1 + GBK）。
3. **PowerShell 会吞掉 cargo 的 stderr** → 统一用 `scripts/cargo-run.mjs` 或 `scripts/verify.mjs`。
4. **管道式 stdio 被禁** → 所有脚本用**文件重定向**。
   ⚠️ 这个坑实际咬过一次：`spawnSync({encoding:'utf8'})` 静默返回空串，让依赖解析「假装成功」。
5. **pnpm 生命周期脚本被禁** → `--ignore-scripts` + 手动 `wxt prepare`；且需 `node-linker=hoisted```。

---

## 已知功能缺口

本节是本轮静态审查（14 项待办）与可重复成就任务的落点。**每一项都能用一条命令复现当前结论。**

| 缺口 | 状态 | 复现 / 证据 |
| --- | --- | --- |
| ~~每日等级与累计成就按单仓库统计（T1）~~ | **已修** | 新增 `daily_global_projection` 表（schema 4）；`load_snapshot` 改读 `global_commit_total()` 与 `daily_global_projection()`；仓库级计数照旧维护。迁移是**加表 + 从既有投影回填**（不重放事件、不撤销解锁），迁移前自动备份。复现：`node scripts/e2e/cross-repository.mjs` → 9 项 PASS（3/2/1 次提交 = 一天 6 次 = Rare）。见 [ADR-0021](docs/decisions/0021-cross-repository-statistics.md) |
| 可重复成就**不补历史**：旧数据库升级后触发表从空开始 | **有意为之** | 迁移是加表，不重放事件、不伪造历史；只有升级后的新有效提交开始计数。复现：`upgrading_v4_to_v5_keeps_the_permanent_unlocks_and_adds_the_trigger_table` → 新表 0 行、永久解锁保留 |
| 每日等级的自定义阈值只有 crate 层能力，设置文件与事件处理未接线 | **未做** | `DailyRankThresholds::custom()` 有 6 个单元测试，但 `Settings` 没有阈值字段，`Engine::evaluate_commit` 仍固定用 `official()`；任务要求「保留现有功能」，因此未擅自改核心统计口径 |
| 9 个内置成就中 5 个不可达：`first_release` / `first_pr_merged` / `streak_7` / `streak_30` / `streak_365`（T2） | **未修** | `first_release`/`first_pr_merged` 的可达性依赖浏览器计数（ADR-0019 已让它们可解锁，见 3b.8）；`streak_*` 仍不可达，因为 `StateSnapshot` 没有连续天数，`engine.rs:225` 硬编码 `longest_streak_days: 0`。**新增成就名单需用户确认** |
| 规范 §22 line 12 的**时间范围 / 仓库 / 稀有度 / 隐藏 / 可重复**五项不在 `RuleExpr` 模型里（T7 相关） | **未做** | `crates/achievements/src/builder.rs` 的 `RuleExpr` 只有 `Condition` / `All` / `Any`，`UserMetric` 四个；`UserAchievement` 没有稀有度与可见性字段。扩模型会改数据库与统计口径，需 ADR 与用户确认 |
| 规范 V2.5 仍写多平台 / Browser Only 三态 / Offer-Claim（T5） | **未做** | 与 ADR-0017（仅 Windows）、ADR-0018（扩展只检测）冲突。规范改动需用户确认 |
| ~~发布构建会用仓库里的开发密钥签名~~ | **已修（T8）** | `DEVCH_RELEASE=1` 时缺 key 或口令直接退出码 7；`node scripts/e2e/release-signing.mjs` 15 项 PASS |
| 浏览器真实链路未在真实浏览器完成端到端（T9） | **部分** | `extension-load` / `native-host` / `browser-to-popup` e2e 已用 CDP 加载未打包扩展并跑通；缺的是**真实 GitHub 提交/合并 PR/发布 Release** 三类事件的现场记录。需要用户配合 |
| Authenticode 代码签名（4b.4） | **受阻** | 无证书；更新签名 `.sig` 不能替代 |
| 发布流水线与多 Git 版本 CI 从未在 GitHub Actions 上运行 | **受阻** | 无仓库。本地已用 PortableGit 2.30.2 / 2.40.1 / 2.54.0 三版本实测（见 ADR-0003） |
| 成就名与等级名未本地化 | **未做** | Rust 侧生成英文文案；等成就目录定稿 |
| ~~`settings.json` 单个坏字段会重置整份设置（丢 `emails`）~~ | **已修（T8）** | 改为逐字段容错 + `.corrupt-<ms>.bak` 备份 + 设置页横幅；8 个单测（`settings::tests::one_bad_field_does_not_lose_the_rest` 等）。复现：`cmd /c "scripts\dev\build-desktop.bat test --bins"` → 51 passed |
| 汇总通知无独立窗口形态，阈值不可调 | **未做** | 见 2a.20；ADR-0020「代价」一节 |
| 播放闸门的时长只认 WAV 头 | **部分** | 导入的 mp3/ogg 交给 `PlaySoundW` 仍能响，但 `audio::wav_format` 只认 RIFF/WAVE；取不到时长时**不设闸门**（等于回到「可能被下一声截断」）。复现：`cmd /c "scripts\dev\build-desktop.bat test --bins"` → `audio::playback_tests::an_unreadable_header_never_blocks_playback`（断言的是「不挂住」，不是「也有闸门」） |

---

## 下一步

**需要用户决定的两件事**（都不阻塞其他工作）：

1. **T2**：新成就名单是否采用建议的 Legendary Day（单日 20 次）、Release Veteran（累计 10 Release）、Team Player（累计 10 PR 合并）、First Issue、First Repository，再加一个隐藏成就？连续天数成就已具备前提（T1 的全局每日投影已就绪）。
2. **T12**：WebView2 安装器（当前仍在版本控制里）如何处理 —— 构建时下载并校验 SHA256，还是 Git LFS？

T1 已于 2026-11-19 获用户批准并完成（迁移方式即「迁移前自动备份 + 从既有投影回填」，不重放事件、不撤销解锁）。

**不需要确认就能继续的**：T5（规范 V2.6 文本）、T9（需要你在沙箱外配合）、T10/T11（文档）、T13 的 CI 矩阵配置、T14 的测试审计。

---

## 变更日志

| 日期 | 变更 |
| --- | --- |
| 2026-11-20 00:55（商店提交材料 + CI 首跑修复） | 提交商店前把外部依赖全部打通。**CI 首跑**：三个 Git 矩阵 job 全部失败，日志显示 `Invoke-WebRequest` 存下的"自解压包"只有几 KB、内容是 GitHub 的 import-map JSON——资产名 `PortableGit-<ver>-64bit.7z.exe` 404 后 GitHub 返回了 HTML 页。对照 release API 核实真名是 `64-bit`，修正并加 10 MB 大小守卫后重跑 **run 44568079398 五个 job 全部 success**（这也是仓库 CI 第一次真正运行）。**隐私政策 URL**：`https://applepinea.github.io/Devchievements/` 此前渲染的是旧 README——Pages 源是 "deploy from branch / root"，pages.yml 的产物根本没被用。把 Pages 源切到 GitHub Actions（API `build_type: workflow`），手动触发 pages.yml；第一次 deploy-pages 因瞬时 API 故障失败，rerun 又因同 run 出现两个同名 artifact 失败，**新开一次 run（44570193529）成功**，页面实测 200 且含 "Privacy Policy" 与联系邮箱。**提交材料汇总**写进 [docs/store/submission-package-2026-11-20.md](docs/store/submission-package-2026-11-20.md)：MSIX（00:31 版，9,162,830 B）+ 隐私 URL + 文案 + 认证备注 + 4 张截图全部就绪；只剩 Partner Center 注册、上传、干净 Win10 三项验证需要用户。**未验证**：干净 Win10 虚拟机三项（真机）；商店审核本身 |
| 2026-11-20 00:31（重新打包 MSIX，商店提交准备） | 工作区干净（HEAD = 8108bce），源码与 23:42 那版包一致，本轮为商店提交把全流程再走一遍并留证据：重新构建 MSVC release（2m 36s，19,823,104 B，13 条 dead_code 警告是既有债务，不是本轮引入）→ `scripts/msix/build-msix.mjs` 11 项全过（含引导器/5 个图标/makeappx/signtool/注册）→ `signtool verify /pa` 成功（时间戳仍为 None，无证书）→ 本地 release 与 WindowsApps 安装副本 **SHA256 一致**（`F60BDE0E…09B865`，19,823,104 B）→ 用**已安装**副本跑 `--preview-sound common`（778 ms 音效）耗时 1188 ms ≥ 838 ms 下限，说明播放闸门在包内生效 → 新包 9,162,830 B 已覆盖 `C:\Documents\Devchievements\Devchievements.msix`。**未验证**：干净 Win10 虚拟机三项（真机才能做）；商店审核口径 |
| 2026-11-20 01:30（可重复成就） | 新增 `commit_complete`：每个有效 Commit 触发一次，独立表按 `(achievement_id, triggering_event_id)` 唯一，同一事件重复导入或并发投递只落一条；永久成就仍按 ID 唯一，每日等级仍在自己的投影/里程碑表。9 个永久成就的 ID、条件、稀有度与 `streak_365` 隐藏属性未改。图鉴新增「可重复成就」区（累计次数 + 逐条触发记录）与「每日等级」当日卡片；普通提交仍只响不弹，可重复触发只计数并保留独立 Outbox 通知。`verify.mjs` 十步全 PASS（500 tests）、桌面 crate 51 项、`cross-repository` e2e 16 项。见 [ADR-0022](docs/decisions/0022-repeatable-achievements.md) |
| 2026-11-19 23:42（重新打包 MSIX） | 提示音修复之后重打 MSIX 并就地升级本机安装：`scripts/msix/build-msix.mjs` 10 项全过；装进 `C:\Program Files\WindowsApps\DevchievementsCompanion_0.1.0.0_x64__zm3808zyk7ry0` 的 `devchievements-desktop.exe` 与本地 release 构建 **SHA256 一致**（`E29487D1…3AC458`，19,820,544 B）。用**已安装**的那份二进制跑 `--preview-sound common`（778 ms 的音效）耗时 1408 ms，说明等待逻辑确实在包内生效。`C:\Documents\Devchievements\` 下的 `.msix`（9,168,333 B）与 NSIS 安装包（7,003,765 B）都已换成新构建，`使用说明.txt` 重写 |
| 2026-11-19（提示音播放器） | 用户报「音频有时无法正常播放，有时只播放出后半部分」，查出**两个独立缺陷**。①**释放后使用**：播放槽是 `Vec<u8>`，旧代码先把新数据拷进槽、再从槽里取指针交给 `PlaySoundW`，于是下一次播放会把系统仍在读的缓冲区释放掉——表现就是「只响半截」或干脆没声。改为 `Arc<Box<[u8]>>` 句柄（`PlaybackBuffer`，地址稳定、随句柄存活），指针取自这块不再移动的分配。②**互相打断**：`PlaySoundW(SND_ASYNC)` 是**替换**当前声音、不能混播，§16.6 也禁止混播；一次 drain 连续投递两条通知时，第二条把第一条截断。改为**播放令牌** `SoundPlayback`：它同时持有进程级闸门、缓冲区句柄和「必须等到放完」的义务，`Drop` 时才等待——所以「忘记等待」不可能发生，而调用方仍能决定**什么时候**等（先记日志、先开窗，再等）。时长从 WAV 头（`fmt ` 的 byte_rate + `data` 长度）算，留 60 ms 尾巴余量；通知与试听按钮走同一个 `start_buffer`，所以试听也不会盖掉通知。顺带修掉一个由本次改动**引入**的语义坑：`SOUND play ... at=` 原来在调用返回后取时间，加了等待之后就变成「声音结束时刻」——现在改成由令牌在**交给设备的那一刻**打时间戳。**实测证据（在最终代码上正反两跑）**：把 `Drop` 里的等待去掉后，`sound-serialized` e2e **失败**（第二个 `SOUND play` 只比第一个晚 **22 ms**，而第一个长 778 ms），`a_second_sound_cannot_start_while_the_first_is_still_playing` 也**失败**（第二个声音只等了 **40 ms**，第一个长 160 ms）；等待加回去后 e2e 晚 **865 ms**、桌面单测 **51 项全绿**。发布二进制另测：`--preview-sound common` 耗时 871 ms（778 + 60 余量 + 启动），`commit` 257 ms。单测 7 项（WAV 时长自解析、截断文件只按实有音频计时、无法解析的头必须立刻返回而不是挂住、已放完的声音不再等待、两次播放不重叠、令牌在第一个声音结束前不放行、缓冲区跨越调用存活）。顺带：`PlaySoundW` 返回失败时现在打印 `PlaySoundW refused the buffer (N bytes)`，不再静默——否则「有时候没声」永远查不到。**未做/未验证**：只解析 WAV 头（导入的 mp3/ogg 会被当成「时长未知」因而不设闸门）、仍用 `PlaySoundW` 而非 WASAPI 会话、多显示器/独占模式未测 |
| 2026-11-19（MSIX 重新打包） | 把本轮改动（跨仓库统计 T1 / 提示音试听 / 主题管理 / 构建器 / CSP / 新鲜度三档）打进商店包：先确认 release 二进制**比源码旧**（源码 22:49–22:50 改过，二进制停在 22:41），重新构建 Vite 产物 + MSVC release 后再执行 `scripts/msix/build-msix.mjs`。10 项检查全过：包 9,167,475 B / 22:58:15，`signtool verify /pa` 成功、0 warning 0 error，`Get-AppxPackage` 已注册，装进 WindowsApps 的 `devchievements-desktop.exe` 与本地 release **SHA256 完全一致**（`5C978D65…AC3CA`，19,819,520 B）。用数据目录副本跑 `--diagnostics`（退出码 0）验证包内迁移：schema 3 → 4、建出 `daily_global_projection`、写出 `backups/pre-migration-v3-to-v4-1795100319802.db`（110,592 B），事件与解锁数不变。交付副本已覆盖 `C:\Documents\Devchievements\Devchievements.msix`（两侧 SHA256 相同）。**未验证**：无 Authenticode 时间戳（`File is not timestamped`）、商店审核口径、Clean 机器免信任证书安装 |
| 2026-11-19（主题管理 + 构建器表单） | 补 T6/T7：主题包校验 crate 一直有测试，但**桌面没有任何入口**，STATUS 却把 4a.3 记为 Tested。新增 `theme_store.rs`（主题存 `themes.json`，与 settings/数据库分开，导入一个主题不会和保存设置打架）、6 个 Tauri 命令 + 6 个 CLI flag、设置页「主题」区（列表/应用/导入/导出/恢复默认/删除）；`theme-manager` e2e 30 项（含脚本包被拒、样式表包被拒、越界值被夹、重启后保留、导出可回导、恢复默认不删用户主题包）。Achievement Builder 页面：表单（commits / PR merged / releases / streak days 四个真实 metric、AND-OR、JSON 双向编辑）+ 本地超限提示 + 已存规则列表/删除；新增 `add_achievement`/`remove_achievement`/`list_user_achievements`/`list_builtin_achievements` 四个命令；`builder-form` e2e 17 项，其中「规则在满足它的那次真实提交上解锁」用真 hook 记录 + 真 companion drain 验证。**未做**：§22 line 12 的时间范围/仓库/稀有度/隐藏/可重复五项在 `RuleExpr` 模型里不存在，未擅自扩 crate schema；导出主题走 `window.prompt` 而不是原生保存对话框 |
| 2026-11-19（弹窗边距 + 设置 BOM） | 用户报「除左上角外其他位置边距调不到 0」。根因**不在**定位算术（那部分三年前就有测试且全绿），而在调用方：`popup::place` 用 `monitor.position()` 当原点（这台机器上恒为 0,0），却用 `SPI_GETWORKAREA` 拿尺寸——`SPI_GETWORKAREA` **只描述主显示器**且**丢掉了工作区自己的 left/top 偏移**。实测用户屏幕工作区是 `{x=72, y=36, w=2488, h=1564}`（150% 缩放），于是右上/右下算出 `1932*1.5=2898` 这类**超出屏幕**的坐标，被末尾的 clamp 压回边缘，表现为「只有左上角是对的」。改为 `Monitor::work_area()`（每显示器独立，且自带原点），并删掉已成死代码的 SPI 辅助函数。实测四角 `margin=0` 现在分别落在 `72,36 / 1959,36 / 72,1449 / 1959,1449`，完全对称。**顺带修掉一个会让用户永远查不出原因的坑**：`settings.json` 若带 UTF-8 BOM（Windows PowerShell 的 `Set-Content -Encoding utf8`、记事本的「UTF-8 带 BOM」都默认写它），`serde_json` 会拒绝整份文档，于是**所有字段静默回退默认值**——从界面看就是「我改了设置它不生效」。已在 `load_with_report` 剥掉 BOM，并加测试 `a_utf8_bom_does_not_discard_the_file` 断言 margin 与自己写的值都存活。新增测试：`a_position_is_relative_to_the_screen_it_is_given_not_to_the_primary_one`、`a_zero_margin_reaches_the_edge_in_every_corner` |
| 2026-11-19（跨仓库统计 T1 + 提示音试听） | **T1**：§14.1 的每日等级与 §13.2 的累计成就改为**跨仓库合计**。此前 A 3 次 + B 2 次 + C 1 次被算成三个独立的 3/2/1（最高 Uncommon），规范要求的是**当天 6 次 = Rare**；Century 同理永远不会在分散提交时解锁，且仪表盘总数（全局求和）与成就进度基数（单仓库）互相矛盾。新增表 `daily_global_projection`（schema 3 → 4），`load_snapshot`/`load_browser_snapshot` 改读全局；仓库级计数照旧维护只供仓库页。状态变更名拆为 `global_commit_total`，存储层用 `+1` 维护 per-repo 列——**这是实现中发现并修掉的真实缺陷**：把全局绝对值写进 per-repo 列会让 `SUM` 重复累加（测出 68920，正确答案 120）。迁移是**加表 + 从既有投影回填**，不重放事件、不撤销解锁；迁移前 `VACUUM INTO` 自动备份到 `<data>/backups/`，备份失败则中止迁移。测试：`the_daily_rank_counts_every_repository_together`、`the_commit_total_counts_every_repository_together`、`a_rebuild_produces_the_same_global_numbers_as_live_ingestion`、`upgrading_an_old_database_backs_up_first_and_keeps_every_unlock`（手工构造 v3 库），e2e `cross-repository.mjs` 9 项。ADR-0021。**提示音试听**：设置页新增每个槽位的试听按钮（commit / 5 个稀有度 / 5 个每日等级），走**真实选择路径**（用户自己的覆盖配置 + 音量），静音或音量 0 时如实报告「选中了什么、为什么没播」而不是假装播放；新增 `preview_sound` / `preview_sound_slots` 命令与 `--preview-sound` CLI；6 个单测 + `sound-preview.mjs` e2e 21 项 |
| 2026-11-19（安全加固 T8） | **CSP**：`tauri.conf.json` 的 `csp` 由 `null` 改为严格策略（`script-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'` 等）；新增 `scripts/e2e/csp.mjs`（8 项）在**真实 WebView2 窗口**里证明注入的 inline script 被拒（附真实拒绝原文），且页面 bundle 仍渲染、inline style 仍生效、应用仍响应。**native-host**：新增 `verify_origin`，只接受固定扩展 ID 的 origin，且在任何 stdin 读取之前执行；5 个单测 + `native-host` e2e 通过。**settings.json**：`load` 改为**逐字段容错**（一个坏字段不再丢掉 `emails`），坏的原文写 `.corrupt-<ms>.bak` 备份，设置页显示告警横幅；7 个单测。**发布签名**：`build-installer.bat` 分 `DEVCH_RELEASE` 与开发两条路径，发布路径缺 key 或口令时**退出码 7 直接失败**、不再静默使用仓库里的开发密钥；新增 `scripts/e2e/release-signing.mjs`（15 项，含「CI 提供的 key 不被覆盖」）。踩坑记录：`exit /b` 写在 `if defined (…)` 括号块里在本机仍上报 0，改成 `goto` 链才拿到 7 |
| 2026-11-19（多 Git 版本证据 T13 + README T11） | **ADR-0003** 由 `Proposed` 改为 `Accepted`，最低版本 **2.30.x**：PortableGit 2.30.2 / 2.40.1 / 2.54.0 各 15/15 场景，2.20.1 连 `git init -b` 都没有；15/15 场景的 hook 顺序 / 事件数 / reflog 文本三版本逐字一致；`post-rewrite` 延迟 388.6–534.8 ms（独立复现 ADR-0004「300 ms 不够」）。**证据边界写进 ADR 本身**：CI 矩阵从未在 GitHub Actions 上运行，2.20.1–2.30.2 之间无数据，单主机单次。另发现并记录**已提交 fixture 05 是 harness 产物**（`GIT_SEQUENCE_EDITOR` 反斜杠路径被 MSYS 吃掉 + `sed -i` 不确定），fixture 与 ADR-0004 数值**未改**，登记为技术债。补 `docs/development/git-version-matrix.md`。**README 重写**：只写已实现的 9 个成就、只写 Windows、无未勾选计划表、无哔哩哔哩/YouTube/学习/探索/社区 |
| 2026-11-19（仓库卫生 T12 + 测试审计 T14） | **T12**：`apps/desktop/src-tauri/null` 删除；`.gitignore` 新增 `apps/desktop/src-tauri/gen/schemas/` 与 `scripts/perf/.first-screen/`；取消跟踪 186 个可再生文件（磁盘副本保留）：`null` 1 + `.wxt` 7 + `gen/schemas` 4 + `.first-screen` 174。**追踪文件 578 → 392**，产物类文件 25 → 1（唯一是 WebView2 安装器，实测 1.81 MiB，与清单一致）。**T14**：把「名字声称无声就必须断言副作用集合为空」写进 [AGENTS.md](AGENTS.md) §10；审查 113 个 `quiet`/`silent`/`no_`/`never`/`not_` 测试，13 个关键项逐个核对断言对象（表见下文「T14 测试名审计」），三处断言不足的已补 |
| 2026-11-19（新鲜度三档真正生效） | 修 T3：§16.3 的中间档（10 分钟–24 小时）此前**仍是完整弹窗**，周末重放 Spool 会弹一串「刚刚解锁」。`NotificationPolicy` 增加 `Digest` 档，`spool_consumer` 三档映射改为**无通配臂**（新档位将编译失败而不是静默回到 Inform）；存储写一行 `kind='digest_summary'`，投递时把一次投递的所有 digest 行折成**一个** `Presentation::Digest`（桌面更新托盘 tooltip、headless 打一行，均不创建窗口、不播庆祝音效）；补 ADR-0020。测试：`the_freshness_bands_decide_what_the_user_sees`（9 分钟 / 11 分钟 / 23h59m / 24h01m）、`a_weekend_replay_produces_no_windows`（50 条重放 0 弹窗 ≤1 汇总）、storage/notifications 各一个单测 |
| 2026-11-18（拒绝即删除） | 查「并没有弹窗」的现场：报告被 §7 身份闸门拒绝后**随 Spool 消费一起删掉**，设置问题变成永久丢数据，UI 上毫无痕迹。新增 schema v3 表 `browser_reports_rejected` 保存原始 payload；设置页「等你处理」列出未关联账号并可一键关联，关联时**写回 Spool 回放**（与扩展投递同一条路径）；`unlinked-login` e2e 15 项 |
| 2026-11-18（过期事件仍弹窗） | 修 §16.3 的执行缺陷：新鲜度原来只在消费端把「排队数」改成 0，**Outbox 行照样写、照样弹**。改为写入端策略（`Inform`/`Quiet`/`Silent`）：超过 24 小时的事件仍然解锁、仍然计统计，但不写 Outbox；解锁来源仍是 realtime。10 分钟–24 小时的「汇总通知」形态仍未实现（已知未做）|
| 2026-11-18（浏览器成就打通） | 修「浏览器报告存进去但什么都不会发生」：Engine 新增 `evaluate_browser_event`（PR Merge / Release / Repository Created / Star Given 各自计数、解锁 §13.2 浏览器成就、**不**移动每日等级）；schema v2 加 5 列并幂等升级旧库；新增 `browser-achievement` e2e 19 项；ADR-0019 |
| 2026-11-18（扩展连接检查） | 逐层验证 Native Messaging：协议、Spool、消费全通；**发现并修复「合并的 PR 被计为一次提交并解锁 First Commit」**；记录扩展未安装、浏览器成就未实现 |
| 2026-11-18（身份映射即时生效） | 修「保存了 GitHub 登录映射仍被拒」：drain 循环原本只在启动时读一次 `settings.json`，改为**每轮重读**（身份 + 自定义成就规则）；实测同一 Spool 记录从 `REFUSED` 变为 `INGESTED 1` |
| 2026-11-18（首屏性能） | 按审查意见先建基线再优化：cold `shell_ready` p50 **848→371 ms**、warm **493→48 ms**；`boot_status` 快速路径、首屏骨架、页面级 lazy、`get_storage_health` 拆分、pool 缓存。setup 阶段实测 0.07/7.26 ms → **未改动** |
| 2026-11-18（首屏缺陷） | 修掉 READ-ONLY 数据跨 IPC 的**线格式不一致**：`Statistics`/`StorageHealth`/`ActivityDay`/`RepositoryEntry`/`Capabilities` 缺 `rename_all = "camelCase"`，导致 `undefined 天`、`schema vundefined`、`Invalid Date`，以及 **About 页白屏**；补 5 个线格式测试 + 页面自检 + `ui-pages` e2e（6 页 × 2 断言） |
| 2026-11-18（性能优化跟进，已并入 main） | Spool 计数改为定长缓冲读取；历史导入去掉未使用的提交主题与全量匹配引用副本；扩展统计改走 IndexedDB 索引。托盘扫描周期试过 100 ms，**实测 p95 437 ms（预算 87%）后回退 50 ms** |
| 2026-11-16（第一轮） | Phase 0 主体：环境勘察、隔离工具链、Monorepo、AGENTS/STATUS/spec 拆分/ADR、15 场景 Git 实验 + fixtures、WASM 验证、扩展构建 |
| 2026-11-18（Phase 4b 第十二轮） | 修掉 §22 的功能缺口：自定义成就**真正会被评估**（同事务、官方统计、复用内置记录路径）；新增 `user-achievements` e2e；顺带让 headless companion 也读规则 |
| 2026-11-18（Phase 4b 第十一轮） | 商店路线打通：MSIX 打包（真图标 / startupTask / 包内 bootstrapper）+ **helper 复制出包**，解决「hook 不能执行包内 exe」；`probe-packaged-app` 10 项实测通过 |
| 2026-11-18（Phase 4b 第十轮） | 依赖检查前移到**安装期**：`installer-hooks.nsh` 校验 WebView2 磁盘文件、内嵌官方 bootstrapper、失败退出码 2 且不留任何文件；`webviewInstallMode` 改为 skip 避免双份逻辑；Git 缺失只警告 |
| 2026-11-17（Phase 4b 第九轮） | 诊断导出补齐 §26.2 的「原始信息 + 明确警告」：`--export-diagnostics [--raw]` + 设置页入口，警告写进文件；WebView2 检测改为**校验磁盘文件**而非只读注册表（虚拟机已证实注册表会撒谎） |
| 2026-11-17（Phase 4b 第八轮） | 修「UI 白屏即可无从诊断」：加 `ui_error` 与全局错误上报（这次就是靠它定位到 TDZ 崩溃）；补无 WebView2 时的原生消息框兜底（`exit(2)`，CLI 豁免）；`webviewInstallMode` 由默认改为显式 |
| 2026-11-17（Phase 4b 第七轮） | 向导改为七步：新增依赖预检（WebView2 注册表 + Git）与浏览器扩展引导；`open_external` 只放行 http(s) 且不弹控制台；修掉「Repositories 页可装 hook」这句不实承诺 |
| 2026-11-17（Phase 4b 第六轮） | 多语言：英文原文作 key + 中文词典（178 条全覆盖），设置页语言选择、`system` 跟随系统；弹窗写死中文的问题一并修掉；新增 `check-i18n.mjs` 与 e2e 语言断言 |
| 2026-11-17（Phase 4b 第五轮） | 修「首启不走向导」：两个 builder 命令清单漂移（真实启动缺 `onboarding_status`/`install_hooks`），改为单一宏；`wizard` e2e 增加真实启动路径与重置流程（18 项）；新增设置页「Re-run setup」与 `--reset-onboarding` |
| 2026-11-17（Phase 4b 第四轮） | 新增「开机自启动」设置：写 `HKCU\...\Run`（卸载器同款名字）、启动时校准、改开关即时生效、`--autostart status/on/off`；判定逻辑测试通过，注册表写入受沙箱限制未验证 |
| 2026-11-17（Phase 4b 第三轮） | 修 release 无控制台启动即崩（日志重定向到 `companion.log`）、内置音效未打包；新增「启动时隐藏到托盘」设置（含配置导出兼容）；桌面启动器改为无终端窗口 |
| 2026-11-17（Phase 4b 第二轮） | §44.1 性能实测：hook p95 10.81 ms、commit→通知 p95 274/328 ms、空闲 CPU 0.12% 整机；修掉 500 ms 轮询导致的超预算（p95 1004 → 274）；顺带发现「自定义成就永不评估」并改记已知问题 |
| 2026-11-17（Phase 4b 第一轮） | 安装包打通（NSIS 工具集预置，签名产物 `.sig`）+ `installer` 真机 e2e；Tauri Updater 接入与数据库快照/回滚（6 个测试）；`release.yml` 发布流水线与工作流 linter |
| 2026-11-16（第二轮） | 补齐 §48.3 风险分析与 §48.5 实施计划；**解除 SQLite/SQLx 与 gix 两项阻塞**；ADR-0002 改为三方案实测；新增 `scripts/verify.mjs` 一键验证（42 测试全绿） |

---

## Phase 4a 任务表（自定义系统）

> 检查点 C 已由用户以「进入 phase4」批准。

| # | 任务 | 状态 | 证据 |
| --- | --- | --- | --- |
| 4a.1 | 资源安全校验（magic bytes / SVG / 配额） | `Tested` | 14 个测试，含伪装成 PNG 的脚本、SVG 执行向量 |
| 4a.2 | Popup Editor 约束与定位 | `Tested` | 15 个测试：范围钳制、四角定位、缩放 |
| 4a.3 | 主题包校验与导入 | `Tested` | 拒绝含脚本/样式/javascript: 的主题；未来 schema 拒绝而非猜测；**设置页「主题」区已接入**（列表/应用/导入/导出/恢复默认/删除），`theme-manager` e2e 30 项 |
| 4a.4 | Sound Manager | `Tested` | 导入音效 + 设置页可管理；`sound-manager` 7 项（含删除后降级不静音）；**每槽位试听**（commit / 5 稀有度 / 5 每日等级）走真实选择路径，静音时如实报告而非假装播放；6 个单测 + `sound-preview` e2e 24 项 |
| 4a.5 | Asset Manager | `Tested` | 导入/删除 + **接入弹窗渲染**；`asset-import` 9 项、`popup` 10 项 |
| 4a.6 | Popup Editor | `Tested` | 宽/缩放/角落/边距/时长/圆角/图标尺寸/透明度/动画/模糊 + 实时预览；`popup` 11 项 |
| 4a.7 | Event Filters 界面 | `Tested` | 设置页可编辑；9 项 e2e 含「不影响官方统计」 |
| 4a.8 | Achievement Builder | `Tested` | 定义校验与存储 12+12 项；用户规则接入评估（同一事务、官方统计、复用内置记录路径）；**Builder 页面已做**：表单（4 个真实 metric / AND-OR / JSON 双向）+ 本地超限提示 + 列表/删除，`builder-form` e2e 17 项（含「真实提交触发解锁」「超限被拒」「删除不撤销已解锁」）。**未做**：时间范围 / 仓库 / 稀有度 / 隐藏 / 可重复 —— `RuleExpr` 模型里没有这些字段，属规范 §22 line 12 与实现的差距，未擅自扩 crate |
| 4a.9 | 配置完整导入导出 | `Tested` | 11 个单元测试 + 18 项 e2e，含「失败不改变现状」 |
| 4a.10 | 可重复成就（Commit Complete） | `Tested` | 独立目录 + 独立表 `repeatable_achievement_events`（schema 5，`UNIQUE(achievement_id, triggering_event_id)`）；每个有效 Commit 一条记录，同一事件重复/并发处理只落一条；通知 ID 为 `ach:commit_complete:<event>`，普通提交仍只响不弹；图鉴分「永久 / 可重复 / 每日」三块并显示累计次数与逐条历史。9 个永久成就的 ID/条件/稀有度未改。见 [ADR-0022](docs/decisions/0022-repeatable-achievements.md) |

---

## Phase 4b 任务表（产品化与发布准备）

> 用户以「继续做phase4b」批准进入。详细过程见
> [phase-4b-progress.md](docs/development/phase-4b-progress.md)。

| # | 任务 | 状态 | 证据 / 缺口 |
| --- | --- | --- | --- |
| 4b.1 | Windows 安装包（NSIS） | `Tested` | 构建出安装包并签名；`scripts/e2e/installer.mjs` 真机装/跑/卸，含「卸载不删数据」 |
| 4b.2 | 更新机制（Tauri Updater，§40.3） | `Partially Implemented` | 插件+公钥+签名产物就位；数据库快照/回滚 6 个测试；**无发布 feed、设置页未接按钮、真实下载安装未验证** |
| 4b.3 | 发布流水线（§40.2） | `Platform Unverified` | `.github/workflows/release.yml` + 工作流 linter；本机无法执行 GitHub Actions，从未真正跑过 |
| 4b.4 | 平台签名（§40.4） | `Blocked` | 无 Authenticode 证书 → 安装包没有代码签名，SmartScreen 会警告；**更新签名（`.sig`）已做，两者不可互相替代** |
| 4b.18 | 浏览器扩展 ↔ 桌面连接（§18） | `Tested`（真机链路见备注） | Native Messaging 三层就位；扩展已用 Edge 未打包加载（id `gfgbmedanlpminnlbbdefojbgphoekbj`），真实 PR 页面报告经 host → Spool → 消费入库（`DRAIN ingested=1 refused=0`，DB 中 `PullRequestOpened` 一行）；**PR 曾被算成提交**已修；浏览器活动现按 §13.2 计数并解锁成就（3b.8）。**未验证**：合并 PR 的真实浏览器事件（需要一次真实 merge）与 VM 上重跑 |
| 4b.17 | 界面数据正确性（§44.2） | `Tested` | 5 个线格式测试钉住 IPC 键名；页面自检把 `undefined`/`Invalid Date` 写进日志；`ui-pages` e2e 逐页打开真实窗口并断言「请求的页 = 渲染的页」且无占位文本（12 项） |
| 4b.16 | 微软商店（MSIX） | `Tested`（本机） | `scripts/msix/` 打真实 MSIX 并装机：安装/AOT 图标/startupTask/数据包外；**helper 复制到 `%LOCALAPPDATA%\bin` 后 hook 可执行并投递记录**（10 项探测全过）。当前用 bootstrapper 按需装 WebView2，认证备注见 `docs/development/store-certification-notes.md`；商店审核口径与无运行库真机流程**未验证**。见 §13–§14 |
| 4b.5 | 数据迁移（§26.4） | `In Progress` | 更新前后快照/还原路径已测；数据库迁移兼容旧版本尚未做 |
| 4b.6 | 多语言（§40.1） | `Tested` | 英文原文当 key + 简体中文字典（178/178）；`system` 跟随 webview locale；`UI LANGUAGE lang=… sample=…` 由 e2e 断言查表真的发生；`check-i18n.mjs` 守漏译 |
| 4b.7 | 性能优化（§40.1 / §44.1） | `Partially Implemented` | hook p95 10.81 ms、commit→通知 p95 274/328 ms、空闲 CPU 0.12% 整机；**首屏已实测并优化**：cold `shell_ready` p50 848→371 ms、warm 493→48 ms（各 12 次，分布不重叠，见 `docs/development/first-screen-performance.md`）。**未测**：首次 onboarding 路径、开窗口时的空闲 CPU、§44.3 并发/损坏场景 |
| 4b.8 | 错误恢复（§40.1） | `Partially Implemented` | 崩溃恢复、更新失败回滚已测；其余错误路径未逐一评估 |
| 4b.9 | 完整隐私设置（§40.1） | `Tested` | 隐私模式/托盘开关/日志脱敏；诊断导出默认脱敏、`--raw` 与设置页勾选才含真实仓库名，**警告写进文件本身**；`diagnostics-export` e2e 9 项 |
| 4b.10 | 启动行为：可选隐藏到托盘 | `Tested` | 设置页开关 + `startHidden`（默认 true）；`STARTUP hidden=` 日志对两种取值都有断言；配置导出对旧文件兼容 |
| 4b.11 | 首次运行向导可达性 | `Tested` | 根因：两个 builder 的命令清单漂移，`onboarding_status`/`install_hooks` 在真实启动里缺失；已收进单一宏 `app_commands!()`。`wizard` e2e 18 项，含**真实启动路径**断言与「重跑向导」 |
| 4b.15 | 安装期依赖安装（§25） | `Tested` | NSIS `PREINSTALL` 钩子：校验 `msedgewebview2.exe` 真实存在（不再信注册表）；缺 WebView2 用内嵌官方 bootstrapper 安装，仍失败则退出码 2 且不装任何文件；缺 Git 先走 `winget`、再下载固定版本官方安装器（`/CURRENTUSER`），失败仅警告；`DEVCH_FORCE_NO_*` 可分别强制两条分支 |
| 4b.14 | 无 WebView2 的原生兜底 | `Partially Implemented` | 建窗口前检查运行库，缺失则弹原生中英双语消息框（含下载入口）并 `exit(2)`；CLI 模式豁免；`--webview2-prompt` 可人工验证。**真正的缺运行库机器未验证** |
| 4b.13 | 启动前置检查 + 扩展引导（§25） | `Partially Implemented` | 向导七步，新增 `checks`（WebView2 注册表 + Git 双路径探测）与 `extension` 引导；`dependencies` e2e 5 项；**缺依赖时的提示文字本机无法验证** |
| 4b.12 | 开机自启动 | `Partially Implemented` | 默认关；写 `HKCU\...\Run\Devchievements`（与卸载器删除的名字一致）；判定逻辑 4 个测试 + `--autostart` 命令；**注册表写入在本环境的沙箱内被拒绝（WIN32_ERROR(5)），真机行为未验证** |

---

## 检查点 C

Phase 3a / 3b 的工作已完成为可验收状态，验收材料见
[checkpoint-c-acceptance.md](docs/development/checkpoint-c-acceptance.md)。

**状态：已批准。** 用户以「进入 phase4」批准了检查点 C，Phase 4a / 4b 已在其后开展。
本节保留为验收材料索引，不再表示「等待确认」。

## 支持范围

**只支持 Windows**（10/11）。macOS 与 Linux 不在支持范围内，
这是产品决定（[ADR-0017](docs/decisions/0017-windows-only.md)），不是遗漏。

CI 只跑 `windows-latest`。规范 §29 的多平台要求据此收敛，
属经授权的偏离。

## 许可证

本项目以 **GPL-3.0-or-later** 发布，全文见 [LICENSE](LICENSE)。

`Cargo.toml` 与 `package.json` 的 `license` 字段与之一致。
