# Phase 2a 进展（本地 Git → 通知的真实闭环）

- **日期**：2026-11-16
- **状态**：**In Progress**
- **验证**：`node scripts/verify.mjs` → **269 tests passed**
- **端到端**：`node scripts/e2e/hook-to-spool.mjs` → **PASSED**

---

## 1. 已完成并**真实验证**的部分

### 1.1 Spool（§10.3–10.6）

追加式本地文件，一行一条记录，带 32 位十六进制校验和。

关键行为都有测试：

| 行为 | 测试 |
| --- | --- |
| 记录往返 | `a_record_round_trips_through_encode_and_decode` |
| 单条损坏不影响其它记录（§10.4） | `a_corrupted_record_does_not_hide_the_valid_ones` |
| 截断行被报告而不是猜测 | `a_truncated_line_is_reported_not_parsed` |
| 满了**拒绝**而不是丢弃（§10.5） | `a_full_spool_refuses_rather_than_drops` |
| 消费是**重命名**不是截断 | `consumers_move_files_instead_of_truncating_them` |
| 并发追加不交叉（§10.6） | `concurrent_appends_do_not_interleave`（8 线程 × 10 条） |

### 1.2 Hook 安装/链式/卸载（§9.4）

| 行为 | 测试 |
| --- | --- |
| 生成的 hook **永不失败提交** | `the_generated_hook_never_fails_the_commit` |
| 已有 hook 被**逐字节**保留 | `an_existing_hook_is_preserved_byte_for_byte_and_chained` |
| 重装不覆盖原备份 | `reinstalling_does_not_clobber_the_original_backup` |
| 用户改过的 hook 卸载时**不动**（§9.4） | `a_user_edited_hook_is_left_alone` |
| 别人的 hook 卸载时不动 | `uninstalling_a_foreign_hook_does_nothing` |
| `core.hooksPath` 被尊重并标记为共享（§9.3） | `a_configured_hooks_path_is_honoured_and_flagged_as_shared` |

### 1.3 IPC 帧协议（§10.2）

长度前缀 + JSON，64 KiB 硬上限，**在分配前**校验声明长度（防恶意/损坏长度）。

### 1.4 端到端（§32.3 的前半段）

`scripts/e2e/hook-to-spool.mjs` 在**真实仓库**上跑：

```text
git commit → .git/hooks/post-commit.exe → devch-helper → Spool
```

实测输出：

```text
  ok   git commit (must not be broken by the hook)
records        : 1
record hook    : post-commit
record sha     : 6c9bef1b017f
record author  : e2e@devchievements.local
E2E PASSED
```

**这不是模拟**：真的 `git commit`，真的 hook，真的 helper 二进制，真的 spool 文件。

---

## 2. 过程中被真实环境逼出来的两个 bug

### Bug 1：hook 根本没跑（sh.exe）

第一版 e2e 用 `#!/bin/sh` 脚本 hook。结果一条记录都没有。
GIT_TRACE 显示 Git **确实**去找了 `.git/hooks/post-commit`，但：

```text
sh.exe: *** fatal error - couldn't create signal pipe, Win32 error 5
```

这是 Phase 0 就记录过的宿主限制：沙箱禁止 MSYS 的命名管道。
改为原生 `.exe` hook 后 Git 能直接执行。

### Bug 2：helper 静默退出

改成 `.exe` hook 后仍然没有记录。原因：Git 用 `.exe` hook 时**不传任何参数**，
而 `run()` 第一行就是 `args.next()?`，于是直接 `None` → 退出码 0 → 什么都没做。

**这个 bug 特别危险**：它表现得像「一切正常」——提交成功、退出码 0、没有报错，
只是永远不产生事件。只有端到端断言 spool 里**有记录**才发现。

修复后 hook 名从自身文件名推断。

---

## 3. 尚未完成（Phase 2a 剩余）

| 项 | 阻塞原因 |
| --- | --- |
| Tauri Companion 与系统托盘 | **无 MSVC**（但**数据闭环已完成**，见 §5） |
| 桌面弹窗与提示音 | 需要 Companion 窗口 |
| 实时 IPC 通道（Named Pipe 服务端） | 需要 Companion 进程 |
| Outbox Consumer | 需要 Companion 循环 |
| Companion 启动消费 Spool | 逻辑可写，但无法端到端验证 |
| 最小诊断视图 | 需要 UI |
| Crash Recovery 矩阵（§32.5） | 需要可强杀的 Companion |

**共同点**：全部需要 Tauri/Companion，而它需要 MSVC。

---

## 4. 一个必须诚实说明的限制

端到端验证走的是 **`.exe` hook 路径**，不是 POSIX shell hook 路径。

- 产品设计（§9.4）生成的是 `#!/bin/sh` 链式脚本；
- 本机**无法运行任何 shell hook**（sh.exe 起不来）；
- 所以「shell hook 链式执行」这件事**从未被真实验证过**。

`hooks.rs` 里的链式逻辑有单元测试（验证生成的脚本内容正确），
但「一个真实的 Husky/Lefthook 环境里链式是否真的被调用」**未验证**。

这一条同时记在 [platform-matrix.md](platform-matrix.md) 与 STATUS。

---

## 5. 本轮：把 Companion 的数据回路做完并真实验证

### 5.1 为什么先做无头 Companion

§32.1 要求的「SQLite 持久化、Engine 调用、Outbox Consumer、启动消费 Spool」
**没有一项需要窗口**。只有最后的弹窗需要。

所以把数据回路做成独立库 `crates/companion-core` + 无头二进制
`apps/companion-headless`，好处是实质性的：

1. **可以真测**：真 SQLite 文件、真 Spool 目录、真记录格式。
2. **崩溃可测**：§32.5 要求 kill 进程。kill 一个无头二进制是常规操作，
   kill 一个 GUI 再断言其内部状态不是。

Tauri 层将来只是「加窗口 + 调用同一组方法」，不重新实现任何逻辑。

### 5.2 完整闭环已跑通

`node scripts/e2e/full-loop.mjs`：

```text
  ok   commit 0/1/2 succeeded with the hook installed
  info helper wrote 3 spool file(s)
  |  DRAIN read=3 ingested=3 dup=0 refused=0 failed=0 queued=6 consumed=3
  |  NOTIFY ach:first_commit:... achievement_unlocked
  |  NOTIFY rank:common:... daily_rank_upgrade
  |  NOTIFY rank:uncommon:... daily_rank_upgrade
  |  DELIVER shown=6 unknown=0
  ok   a second run reads nothing new
FULL LOOP PASSED
```

真仓库、真 `git commit`、真 helper、真 Companion、真 SQLite。
**唯一替换的是渲染层**（stdout 而非窗口），因为窗口需要 MSVC。

### 5.3 Crash Recovery 矩阵（§32.5）部分完成

`node scripts/e2e/crash-recovery.mjs` → **CRASH MATRIX PASSED**（12 项检查）

| 场景 | 结果 |
| --- | --- |
| 1. Hook 写入 Spool | ✅ 计数一次，文件被消费 |
| 3. 事务提交前中断 | ✅ 重放被判重，不重复计数 |
| 4. 提交后、消费前中断 | ✅ 重启不重复投递 |
| 6. 展示尝试已持久化、完成丢失 | ✅ 恢复为 unknown，**不再展示** |
| 2. IPC 已到达但确认丢失 | ❌ 未覆盖（需要实时 IPC 通道） |
| 5. Outbox 消费中 | ❌ 未覆盖（需要可强杀的窗口进程） |

**未覆盖的两项是明确写出来的，不是悄悄跳过。**

### 5.4 又是同一个坑：管道捕获

第一版 crash 脚本全部失败。原因是 `spawnSync({encoding:'utf8'})` 用管道捕获输出，
而沙箱禁止管道——拿到空字符串，于是所有正则都不匹配。

**这是本项目第三次被同一个限制咬到。** 已全部改为文件重定向。

还有一个更隐蔽的：生成的 JS 文件里 `\d` 反斜杠丢失，变成字面量 `d`，
正则永远不匹配。改为不用正则解析。


---

## 6. 本轮：实时 IPC 通道

### 6.1 实现

用 `interprocess` 2.4.8（命名管道 + domain socket 一套 API，自己的 crate 里不需要 `unsafe`）。
客户端在 Helper，服务端在 Companion；服务端**先把记录写进 Spool，再回 ack**，
所以 ack 的含义是「已持久化」，不是「大概收到了」。

### 6.2 🔴 本机无法验证：命名管道被沙箱拒绝

同进程内绑定后立即连接：

```text
connect failed: 拒绝访问。 (os error 5)  kind=PermissionDenied
```

与 Phase 0 的 `sh.exe` 失败同源（`Win32 error 5`，signal pipe）。
**结论：实时通道是「编译通过但未验证」。**

### 6.3 修掉一个真实缺陷：确认没有超时

第一版 `deliver_live` 在读确认时不设超时。若服务端接受连接却不应答，
Helper 会**无限阻塞**，直接违反 §9.2。

改成「读在独立线程 + 主线程 `recv_timeout`」后，等待才真正有界。
并被一条测试锁住：通道不可用时投递必须在 500 ms 内失败。

**这个缺陷是端到端测试永远发现不了的** —— 端到端总是走回退路径。
是写自测时进程挂住才暴露。这正说明为什么「能编译」和「能跑通流程」都不等于正确。

### 6.4 已验证 / 未验证

| 项 | 状态 |
| --- | --- |
| 通道不可用时快速失败 | ✅ |
| 通道不可用时回退 Spool | ✅ 端到端 |
| Companion 在线/离线都不丢事件 | ✅ 端到端 |
| 回退记录能被 Companion 消费 | ✅ 端到端 |
| **实时 ack 路径** | ❌ 未验证（环境限制） |
| 跨用户连接攻击 | ❌ 未验证（单用户会话） |

两个真实往返测试被 `#[ignore]`，理由写在测试名里，不用静默 skip 掩盖。


---

## 7. 本轮：Privacy Mode 基础行为（§26.2）

新建 `crates/privacy`，26 个测试。选它是因为它是**纯逻辑**：
不依赖窗口，可以完整验证。

### 7.1 最容易做错的一条

§26.2 说 Privacy Mode「只显示成就名与通用文案」。
所以隐私模式**不是静音**——成就名要保留，只藏上下文。

如果实现成「什么都不显示」，用户会以为成就系统坏了。
有一条测试专门锁这个：标题必须等于成就名，正文必须是通用文案。

### 7.2 稳定别名，而不是「(已隐藏)」

§26.2 要求诊断包导出时「仓库名替换为稳定别名」。
用 `(hidden)` 会让诊断信息失去全部价值——两条记录无法区分。

所以别名是确定性的哈希：同一仓库永远同一别名，不同仓库不同，
且不包含原始名称的片段。

### 7.3 原始导出必须显式确认

`resolve_export_mode` 不接受「传个 flag 就导出原文」。
必须同时传入用户看到的那段警告文本，否则拒绝。
这样「忘记警告」变成一个显式选择，而不是手滑。

### 7.4 日志结构上就不带 Commit Message

`LogLine` 的 `commit_message` 字段是私有的，
只能通过 `with_commit_message()` 添加——§26.2 要求日志默认不含提交信息。

### 7.5 测试抓到的真实 bug

`redact_path("/home/alice-two/work", Some("/home/alice"))`

第一版用 `starts_with` 判断主目录前缀，于是把**另一个用户**的路径
折叠成了 `~-two/work`。这会让诊断包把两个人的路径混为一谈。

修复：前缀必须终止于路径分隔符或字符串末尾。


---

## 8. 本轮：通知队列与合并（§16.4、§16.6）

### 8.1 发现的问题

`merge_notification` 早就写好了，也有单元测试，但**没有任何地方调用它**。
真实流程里每个 outbox 行都各自弹一次窗。

也就是说：一个首次提交会**弹三个窗**（成就 + 提交 + 每日等级）。

同一个提交还违反了 §16.4：「普通 Commit 轻提示音，不弹完整通知」——
实际会弹一个完整的提交通知。

### 8.2 修复

合并是**展示层**的职责，所以放在投递阶段而不是存储阶段：

- 存储仍然一意图一行（每一行是独立的状态机条目，崩溃恢复要用）；
- 投递时按 `event_id` 分组，合并成一个展示。

新增 `Presentation`：`Popup` / `SoundOnly` / `Nothing`。

### 8.3 端到端实测输出

```text
DRAIN read=3 ingested=3 queued=6
NOTIFY popup title="First Commit" rarity=common others=["Common Day"] omitted=0
NOTIFY popup title="Uncommon Day" rarity=uncommon others=[]
SOUND commit
DELIVER shown=3 unknown=0
```

三个提交、六个 outbox 行、三次展示：

1. 首次提交：成就 + 每日等级**合并为一个弹窗**，等级作为副行；
2. 第二个提交升到 Uncommon：单独弹窗；
3. 第三个提交没有新解锁：**只出声，不弹窗**。

### 8.4 我的测试又写错了两处

- **fixture 名冲突**：新测试用了 `"merge"`，和已有的
  `a_merge_record_is_recorded_but_not_counted` 撞车，两个测试并行互删目录。
  失败信息是「pending 为 0」，看起来像代码 bug，其实是测试隔离问题。
- **前提错了**：我以为「同一天第二个提交不会升级」，
  但 1 → 2 个提交正好是 Common → Uncommon。要用第三个提交才落在同一等级内。

两处都是测试的错误，不是实现的错误 —— 但如果不写这两个测试，
合并功能就会一直「看起来对」。

---

## 9. 本轮：MSVC 到位，桌面外壳跑起来了

### 9.1 先确认「装好了」是什么意思

MSVC 装好 ≠ 能用。实测：

| 检查 | 结果 |
| --- | --- |
| `vswhere` | **不存在**（所以一开始看起来像没装） |
| `cl.exe` | 在 VC Tools MSVC 14.51.36231 的 Hostx64/x64 |
| Windows SDK | 10.0.26100.0 ✅ |
| WebView2 运行时 | 157.0.4341.46 ✅ 已安装 |
| Rust MSVC 工具链 | **需要单独装**（原来只有 GNU） |

### 9.2 三个坑

**坑 1：Git 的 link.exe 冒充链接器。**
hermes 里 Git 自带的 `usr/bin/link.exe` 是 GNU 的 link 工具，
它在 PATH 里排在 MSVC 前面，报错会指向完全错误的方向。

**坑 2：批处理里 (x86) 提前闭合 if 块。**
我自己写探针时踩到的，报错是 `Microsoft was unexpected at this time.`，
和真实原因毫无关系。规则：含路径的东西不要放进括号块。

**坑 3：verify.mjs 的环境把 vcvarsall 搞坏了。**
为 GNU 步骤重制的 PATH（含 `CC=mingw gcc`）传给 .bat 后，
vcvarsall 探测不到 SDK，表现为 windows.h / malloc.h 找不到，
**但 MSVC 自己的头文件在**。修法：桌面构建步骤用原样 environment。

### 9.3 一个真正的代码缺陷

Tauri 命令要求 `Send`，而 `drain_once` 跨 `await` 持有
`&dyn ClockProvider` 等 trait 对象，整个 future 不是 `Send`。

无头二进制用单线程执行器，所以**从来没暴露**。
给四个 provider trait 加 `Send + Sync` 后解决 —— 它们本来就该是这个约束。

### 9.4 WebView2 起不来，但要区分清楚

- Tauri 建窗口时 panic：`Failed to setup app: 拒绝访问。(os error 5)`
- `msedgewebview2.exe --version` → 退出码 13，无输出

**但普通 Win32 窗口是允许的**（WinForms 探针成功创建并显示）。
所以限制是「不能有 WebView2」，不是「不能有窗口」。

因此应用改成**默认只建托盘** —— 这正好也是 §32.1 要的形态。

### 9.5 端到端实测

`node scripts/e2e/desktop-tray.mjs` → **PASSED**

```text
  ok   the desktop app started and created its tray
  ok   commit 0 / 1 / 2 succeeded
  |  TRAY READY
  |  IDENTITY from settings.json
  |  INGESTED 2
  |  POPUP First Commit [Common] ["Common Day"]
  |  POPUP Uncommon Day [Uncommon] []
  |  INGESTED 1
  |  SOUND commit
  ok   it merged the first commit into one popup
  ok   it acknowledged a plain commit by sound
```

真实 Tauri 进程、真实提交、真实 helper、真实 SQLite。

### 9.6 顺带补上的真实缺口

第一版桌面端把身份硬编码成 `unknown@example.com`，
于是所有真实提交都被身份门禁拒绝 —— **门禁是对的，应用是错的**。

改成读 `settings.json`，缺失时回退到 Git 配置建议，
两者都没有就拒绝统计并打印原因（§7）。

### 9.7 仍未验证

- **自定义通知弹窗**：需要 WebView2。
- **Windows Toast**：非打包进程无包标识（`0x80073D54`），
  与本沙箱无关，但需要注册 AppUserModelID + 开始菜单快捷方式，尚未实现。

---

## 10. 本轮：Windows Toast（安装器注册，应用只检查）

### 10.1 为什么分成两件事

非打包 Win32 进程没有包标识，Toast API 一律拒绝（`0x80073D54`）。
这是**所有非 MSIX 程序**的默认行为。

微软的要求是两条同时满足：

1. 开始菜单里一个快捷方式，属性 `System.AppUserModel.ID` 指向我们的 AUMID；
2. `HKCU\Software\Classes\AppUserModelId\<id>` 提供显示名。

**这两件事都是安装行为，不是运行时行为。** 每次启动都去改开始菜单和注册表的程序，
和恶意软件的行为没有区别；普通用户账户往往也没有权限。

所以：

| 角色 | 职责 |
| --- | --- |
| `scripts/install-companion.ps1` | 注册（写快捷方式、写注册表、通知 shell） |
| `scripts/uninstall-companion.ps1` | 反向清理（不动用户数据） |
| 应用 | **只读检查**（`--toast-status`），不写任何系统位置 |

### 10.2 安装器实测输出

`powershell -File scripts/install-companion.ps1` → **exit 0**

```text
install: writing the shortcut
REGISTER OK ...\install-staging\Devchievements.lnk
install: copying to C:\Users\csa\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Devchievements.lnk
install: registering the display name
install: notifying the shell
SHELL NOTIFIED
install: verifying
install: System.AppUserModel.ID = [Devchievements.Companion]
{"detail":"registered","ready":true}
install: done
```

应用侧：`--toast-status` → `{"detail":"registered","ready":true}`（退出码 0）。

### 10.3 逐个排除的记录

| 尝试 | 结果 |
| --- | --- |
| 只写注册表键 | ❌ `0x80073D54` |
| 再加 `SetCurrentProcessExplicitAppUserModelID` | ❌ `0x80073D54` |
| 完整注册（键 + 快捷方式 + 进程 AUMID + shell 通知） | ❌ 仍 `0x80073D54` |
| 用 PowerShell 以**它自己的身份**调用 | ✅ 弹出成功，通知中心可查 |

第一条的教训：**PowerShell 那次「成功」不能证明注册表够用**，
因为 powershell.exe 自身就带 AUMID。我一开始据此下了错误结论，后来才纠正。

### 10.4 🔴 结论：本机无法验证

注册的每一步都成功、每一步都能被外部读到，Toast 平台仍然拒绝。

这不是缺少代码路径，而是**这台机器上只有 Windows 已经信任的进程**（如 PowerShell）
能使用 Toast。具体原因尚未查明（可能是安全软件，**未验证**）。

因此 `scripts/e2e/toast.mjs` 用三种退出码区分：

| 码 | 含义 |
| --- | --- |
| `0` | 真的弹出且出现在通知中心 |
| `1` | 实现坏了 |
| `2` | 环境阻止了验证 |

当前结果 `2`。**按 AGENTS.md §10，STATUS 中只能是 `Implemented`，不是 `Tested`。**

### 10.5 顺带修掉的环境坑

加 Toast 依赖后**连 hello world 都链接失败**：`LNK1104 lnk{GUID}.tmp`。

根因与依赖无关：**TMP 指向 `%LOCALAPPDATA%\Temp` 时，孙进程
（cargo → link.exe）写不进去**。同一个坑也让 `std::env::temp_dir()` 的测试集体失败。

已固化：

- `scripts/dev/build-desktop.bat` 自己设 TEMP 到工作区内；
- `scripts/verify.mjs` 给所有子进程设 `.verify/tmp`。

---

## 11. 本轮：提示音（§19）

### 11.1 分工

| 层 | 职责 |
| --- | --- |
| `crates/sound` | **选择逻辑**：优先级与回退，纯逻辑，无 IO |
| `apps/desktop/src-tauri/src/audio.rs` | 把 id 映射到文件并播放 |
| `scripts/generate-sounds.mjs` | 生成内置音效（可复现） |

选择逻辑做成纯函数，是为了**没有声卡也能测优先级规则**——
而规则恰恰是最容易写错的部分。

### 11.2 实现中的一个真实 bug

第一版 `first_available` 把「稀有度内置音」当成了「成就专属槽的回退」，
于是**没有成就专属音效时会立刻返回稀有度内置音**，
后面的每日等级/稀有度/全局自定义音全都被跳过。

3 个测试当场失败（期望 `custom-daily`，得到 `rarity-epic`）。
修法：每个候选槽位区分「自定义音」和「该槽自己的内置音」，成就槽没有内置音，
所以它不该拦截后续槽位。

### 11.3 一处规范没写明的判断

§19 只给了类别优先级，没说「稀有度内置音」与「全局自定义音」谁优先。

我选了**保持类别顺序**：稀有度内置音优先于全局自定义音。
理由：若任何自定义音都压过任何内置音，用户只设一个全局音，
就会失去全部稀有度差异，那等于取消了稀有度音效。
这一条写进了代码注释和测试名（`a_rarity_built_in_outranks_a_global_custom_sound`）。

### 11.4 实测输出

`node scripts/e2e/desktop-tray.mjs` → **PASSED**

```text
POPUP First Commit [Common] ["Common Day"]
SOUND play id=rarity-common from=BuiltIn
POPUP Uncommon Day [Uncommon] []
SOUND play id=daily-uncommon from=BuiltIn
SOUND play id=commit from=BuiltIn
```

对应 §19 的三条路径：成就弹窗用稀有度音、每日等级用每日音、普通提交用提交音。

### 11.5 顺带修掉的回归

重构安装器命令时，投递出口的 **Toast 调用被整段丢掉过一次**。
这次重写出口时补回，并由 `TOAST READY` / `TOAST FAILED` 日志可见。

---

## 12. 本轮：换成指定音效 + 最小诊断视图数据侧

### 12.1 音效调整

按用户提供的 `achievement.wav`（0.78 s，48 kHz 立体声 16-bit）替换默认音：

| 槽位 | 音效 |
| --- | --- |
| 奖励（任意稀有度 / 每日等级） | `achievement`（用户提供） |
| 普通提交 | `commit`（合成的 55 ms 轻提示音） |

**保留提交音不变是刻意的**：§16.4 要求普通提交是「轻提示音」。
每次提交播一段 0.78 秒的成就音，正是那条规则要避免的打扰。

稀有度/每日等级的**分档音效仍然是能力**——选择逻辑没有删，
只是这套内置资源共用一个文件。要恢复分档，改 `REWARD_SOUND` 一处即可（Phase 4a 的 Sound Manager）。

同时修掉一个资源管理隐患：构建脚本原来只做 `xcopy`，
**删掉的音效不会从产物里消失**。改成先删目录再复制。

### 12.2 最小诊断视图（§32.1）数据侧

新增 `crates/storage/src/diagnostics.rs` 与 `--diagnostics` 命令，输出真实 JSON：

```json
{
  "dataDir": "~/Documents/.../data/devchievements",
  "snapshot": {
    "events": 3,
    "notifications_never_shown": 0,
    "outbox": [{ "count": 6, "status": "completed" }],
    "repositories": [{ "alias": "repo-dd5e0648c95dc648c41e", "commits": 3 }],
    "schema_version": 1,
    "unlocked_achievements": ["first_commit"]
  },
  "sounds": ["achievement", "commit"],
  "spool": { "pendingBytes": 0, "pendingRecords": 0 },
  "toast": { "detail": "registered", "ready": true }
}
```

关键点：

- **仓库名是稳定别名**（§26.2），原始 id 不出现在任何可分享的输出里；
- **数据目录被折叠**成 `~`，用户名不泄漏；
- `notifications_never_shown` 单独统计——这是用户真正会抱怨的失败模式（§17.9）。

### 12.3 写 e2e 时踩的两个坑（都是我的错）

1. **脚本里误启动了一个永不退出的托盘进程**，导致整个 e2e 变成后台任务挂住。
2. **git 提交没带 `LOCALAPPDATA`**，helper 把记录写进了真实用户目录，
   于是 companion 读到 0 条——看起来像摄取坏了，其实是测试自己写错了地方。

第 2 条尤其值得记：它会让「测试通过」建立在污染真实环境上。

---

## 13. 本轮：托盘真实动作 + 隐私模式快捷入口

### 13.1 托盘菜单

| 项 | 行为 |
| --- | --- |
| Privacy mode（勾选） | 切换并**持久化**到 `settings.json`，重启后仍生效 |
| Run one drain | 立刻唤醒摄取循环，而不是等满 500 ms |
| Quit | 退出 |

§26.2 明确要求「提供手动开关与托盘快捷入口」，所以隐私模式做成了**勾选项**而不是一次性动作：
用户打开后重启，它必须还是开着的。

### 13.2 隐私模式的实际效果

同一个提交，两种设置下通知文案不同：

```text
privacy=normal   COPY privacy=false title="First Commit" body="Common Day"
privacy=privacy  COPY privacy=true  title="First Commit" body="A new milestone has been reached"
```

成就名保留、上下文被替换——这正是 §26.2 规定的形状。
文案由 `devchievements-privacy` 生成，规则只有一处实现、24 个测试。

### 13.3 新增设置文件

`settings.json`（`apps/desktop/src-tauri/src/settings.rs`）：

```json
{ "schemaVersion": 1, "emails": ["..."], "privacy": "privacy" }
```

为什么是文件而不是数据库表：身份门禁需要在**打开数据库之前**读到邮箱，
而表要求数据库先存在——那是循环依赖。

### 13.4 又一次「第一处匹配」的坑

我用字符串替换插入「启动时读取隐私模式」，结果 `let toast_state = ...` 这一行
在 `--diagnostics` 分支里也出现过，替换落在了**错误的那个**上：
隐私值只在 diagnostics 分支被设置，托盘启动路径从未读取，
于是日志一直打 `privacy=false`。

修复方式不是猜，而是把两处出现都打印出来对比上下文，
再用「托盘特有的后续代码」作为锚点插入。

这类错误 e2e 会抓到，但它伪装成「功能没实现」，而不是「插入位置错了」。
