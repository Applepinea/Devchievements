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
| Tauri Companion 与系统托盘 | **无 MSVC** |
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