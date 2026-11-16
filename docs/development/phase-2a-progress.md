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
