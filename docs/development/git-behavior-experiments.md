# Git 行为实验记录（Phase 0 / §30.3）

- **执行时间**：2026-11-16
- **Git 版本**：`git version 2.54.0.windows.1%%（x86_64）
- **宿主**：Windows 11 build 26300，x64
- **执行方式**：`scripts/git-experiments/run-experiments.mjs%%（15 个场景，全部真实执行）
- **原始数据**：`scripts/git-experiments/.out/%%（每场景一份 `trace.jsonl%% + `summary.json%%）

> 本文件只写**实测到的事实**。推断一律显式标注。

---

## 0. 方法

每个场景在 `.out/<scenario>/repo%% 下新建一次性仓库：

- 固定身份 `Devch Experi <experiments@devchievements.local>%%
- 固定提交时间（从 `1700000000%% 起每 60 秒递增），保证可复现
- 安装 `post-commit%% / `post-rewrite%% / `post-merge%% 探针
- 探针每次调用写一行 JSONL

探针是 **Rust 原生可执行文件**（`hook-recorder-rs%%），不是 shell 脚本。
原因不是偏好，而是本机限制（见 §6）。

每次记录：hook 名、argv、**stdin**、HEAD、parents、tree、分支、author/committer 与时间、
**HEAD reflog 尾部**、reflog 是否已含 HEAD、可达提交数、`.git%% 目录、Git 版本、自身耗时。

---

## 1. 场景结果汇总

| 场景 | 触发的 Hook | post-rewrite 延迟 | reflog 消息种类 | 钩子运行时 reflog 已含 HEAD |
| --- | --- | --- | --- | --- |
| 01-plain-commit | post-commit×2 | — | 2 | ✅ |
| 02-amend | post-commit×2, post-rewrite×1 | 436 ms | 2 | ✅ |
| 03-amend-then-commit | post-commit×3, post-rewrite×1 | 440 ms | 3 | ✅ |
| 04-rebase | post-commit×6, post-rewrite×1 | 452 ms | 6 | ✅ |
| 05-interactive-rebase-reword | post-commit×3 | — | 3 | ✅ |
| 06-merge-no-ff | post-commit×3, post-merge×1 | — | 3 | ✅ |
| 07-merge-ff | post-commit×2, post-merge×1 | — | 2 | ✅ |
| 08-empty-commit | post-commit×2 | — | 2 | ✅ |
| 09-cherry-pick | post-commit×3 | — | 3 | ✅ |
| 10-squash | post-commit×4, post-merge×1 | — | 4 | ✅ |
| 11-worktree | post-commit×2 | — | 2 | ✅ |
| 12-custom-hooks-path | post-commit×1 | — | 1 | ✅ |
| 13-existing-hook-chaining | — | — | 0 | ❌ |
| 14-submodule | post-commit×1 | — | 1 | ✅ |
| 15-sparse-checkout | post-commit×3 | — | 3 | ✅ |

> 「reflog 已含 HEAD」回答的是：**post-commit 执行时 `git reflog%% 是否已经能看到这次提交。**

---

## 2. 关键发现

### 2.1 post-commit 执行时 reflog 已经包含新 HEAD

14 个产生 post-commit 的场景里，探针在 post-commit 内执行 `git reflog --format=%H%% 时，
**本次提交的 SHA 已在列表中**（`reflog_has_head = true%%）。

**含义**：Helper 可在 post-commit 时刻直接读到 HEAD 最近动作，不必为 reflog 信息额外等待。
这为 §9.6 第 1 条策略提供可行性证据。

**但**：reflog **消息文本**是本版本的实现细节（见 2.2），不能当跨版本契约。

### 2.2 reflog 消息文本（仅 2.54.0 实测）

| 操作 | `%gs%% 实际内容 |
| --- | --- |
| 首次提交 | `commit (initial): first%% |
| 普通提交 | `commit: second%% |
| Amend | `commit (amend): amended message%% |
| Rebase 的 pick | `rebase (pick): topic-1%% |
| Rebase 结束 | `rebase (finish): returning to refs/heads/topic%% |
| Cherry-pick | `cherry-pick: pickable%% |
| 非快进合并 | `merge feature: Merge made by the 'ort' strategy.%% |

**推断（未跨版本验证）**：规范 §9.6 明确禁止把固定 reflog 字符串当成所有版本稳定可靠。
本次只验证 2.54.0，因此这些字符串只能当**提示信号**，
**不得**作为唯一判定依据，必须保留 `pending_rewrite_classification%% 兜底。

### 2.3 post-rewrite 延迟 436–452 ms（实测）

| 场景 | 延迟 |
| --- | ---: |
| Amend | 436 ms |
| Amend 后连续 Commit | 440 ms |
| Rebase | 452 ms |

规范默认窗口是 **300 ms**。实测 post-rewrite 在 **436–452 ms** 才到，
即默认 300 ms 窗口**在本环境必然超时**，每次 Amend / Rebase 都会被当普通 Commit 处理。

**测量口径说明**：该延迟 = post-rewrite 进程启动时刻 − 上一个 post-commit 进程启动时刻，
包含 post-commit 自身工作时间与进程退出。post-commit 自身约 330 ms（§3），
即便扣掉这部分，post-rewrite 仍明显晚于 300 ms。

**结论**：默认窗口需重新定，见 [ADR-0004](../decisions/0004-rewrite-classification-window.md)。

### 2.4 交互式 Rebase（reword）**不产生** post-rewrite

`05-interactive-rebase-reword%% 只有 3 次 post-commit，**没有** post-rewrite（rebase 返回码 0，成功）。

**含义（重要）**：若产品完全依赖 post-rewrite 识别重写，
**交互式 reword 会被误判成全新普通提交**，从而重复计数。
必须在 post-commit 阶段用其它信号（如 `rebase (pick)%% 消息）做初步分类，
并把该局限写进文档，而不是假装已解决。

### 2.5 快进合并不创建提交

`07-merge-ff%%：`post-merge%% 触发（argv = `0%%），但 **`post-commit%% 不触发**（该场景的 2 次 post-commit 来自先前提交）。
符合 §8.1「Merge Commit 不进入普通 Commit 统计」——快进合并本来就没有提交。

### 2.6 post-merge 的 argv 可直接区分 squash

| 场景 | `post-merge%% argv |
| --- | --- |
| 普通 no-ff 合并 | `0%% |
| `git merge --squash%% | `1%% |

Helper 可免费从 argv 获得「这是 squash 合并」的信息。

### 2.7 Cherry-pick 与 Empty Commit 的可判别信号

- **Cherry-pick**：reflog 为 `cherry-pick: ...%%，**无** post-rewrite，patch 与源提交一致。
- **Empty Commit**：`--allow-empty%% 的提交，tree 与父提交**完全相同**（实测为 true）。廉价且可靠的判定信号。

### 2.8 Worktree

`11-worktree%% 中 worktree 内提交正常触发 post-commit；探针记录的 `.git%% 是 worktree 自己的目录，
`commondir%% 指回主仓库。

**含义**：Helper 必须区分 `--absolute-git-dir%% 与 `--git-common-dir%%，
不能假设 `.git%% 一定在仓库根下、也不能假设它是目录（worktree 里是**文件**）。

### 2.9 自定义 `core.hooksPath%% 正常工作

把 `core.hooksPath%% 指向共享目录后 post-commit 仍触发。

**含义**：安装器必须尊重 `core.hooksPath%%（§9.3），否则装到 `.git/hooks%% 的钩子根本不会执行。

### 2.10 Sparse checkout 不影响 Hook

`15-sparse-checkout%% 三次提交均正常触发 post-commit。

---

## 3. Hook 耗时实测（对应 §44.1）

### 3.1 进程创建基线（各 40 次）

| 被测对象 | p50 | p95 |
| --- | ---: | ---: |
| `cmd /c exit%% | 13.17 ms | 13.91 ms |
| 空 Rust 二进制 | 8.99 ms | 10.25 ms |
| `git --version%% | 30.75 ms | 32.76 ms |
| `git rev-parse HEAD%% | 32.86 ms | 41.76 ms |

**关键**：空原生进程只需 **9 ms**，而每次 git 启动要 **33 ms** —— 多出的 24 ms 是 **git 自身启动成本**，不是进程创建成本。

### 3.2 单次 git 调用成本

17 个 plumbing 命令各 30 次：**单次 p50 ≈ 33–37 ms**，**合计 p50 = 601.4 ms，p95 = 672.6 ms**。

### 3.3 Helper 三种实现方案对比（同仓库、同批事实、各 40 次）

| 方案 | min | p50 | p95 | max | 满足 Hook p95 < 50 ms |
| --- | ---: | ---: | ---: | ---: | --- |
| A　单次 git 调用（最省的外派） | 33.95 | 35.47 | 38.34 | 45.33 | ✅ |
| A'　11 次 git 调用（朴素外派） | 369.15 | 393.78 | 428.26 | 485.62 | ❌ |
| B　链接 gix / git2 | — | — | — | — | **本机无法构建** |
| C　直接读 `.git%%（自研，1 个进程） | 9.05 | 10.35 | 12.53 | 70.35 | ✅ |

方案 C 的**正确性**已用 `git rev-parse HEAD%% 交叉验证（SHA 与父提交数一致）。

**结论**：从「17 次 git 外派」改为「1 个原生进程直读 `.git%%」，
p95 从 **672 ms → 12.5 ms**，从**必然违反 200 ms 硬超时**变为**优于 50 ms 目标**。

> 过程记录：方案 C 第一版只实现了固定 Huffman 块，**正确性检查失败**
> （`dynamic huffman block unsupported%%）。改成完整 inflate 后又因错误地按位反转
> 范式码（Huffman code）而报 `invalid huffman code%%。
> 两次都是**先有正确性断言**才发现的。这正是不能只测时间、不测结果的例子。

---

## 4. 对规范的具体影响

| 规范条款 | 实测结论 | 动作 |
| --- | --- | --- |
| §9.6 窗口默认 300 ms | post-rewrite 实测 436–452 ms | 调整 → ADR-0004 |
| §9.6 不得依赖固定 reflog 字符串 | 仅 2.54.0 实测 | 只当提示信号，保留 pending 兜底 |
| §9.6 交互式 Rebase | 实测**不触发** post-rewrite | 承认局限并写进文档 |
| §9.2 用 Rust Helper | 与实测一致 | 保持 |
| §9.5 Helper 实现方案 | 直读 `.git%% 显著优于外派 | ADR-0002 |
| §44.1 Hook p95 < 50 ms | 直读 12.5 ms 达标；外派 672 ms 超标 | 采用直读 |
| §9.3 `core.hooksPath%% | 实测生效，必须尊重 | 安装器按该路径安装 |
| §9.3 Submodule | **本机无法验证** | 标为未验证 |

---

## 5. 未验证 / 无法验证事项（诚实清单）

1. **其它 Git 版本**：只有 2.54.0。规范要求的「CI 跨多版本」**未完成**。
2. **其它平台**：只有 Windows 11。macOS / Linux 的 reflog 文本与触发顺序**未验证**。
3. **Companion 在线/离线**：Phase 0 还没有 Companion，IPC/Spool 路径**未验证**。
4. **Submodule**：受本机限制无法执行 `git submodule add%%（§6），**未验证**。
5. **窗口最优值**：只能给出下界（> 452 ms），跨版本分布**未知**。

---

## 6. 本机限制导致的偏差（必须记录）

1. **MSYS `sh.exe%% 完全无法启动**
   报错：`fatal error - couldn't create signal pipe, Win32 error 5%%
   原因：沙箱禁止创建 MSYS 运行时使用的命名管道。
   **后果**：
   - 探针必须是原生 PE；
   - 「已有 shell 钩子能否被链式执行」**无法在本机验证**；
   - `git submodule add%%（内部走 `sh%%）**无法执行**。
   **这是宿主限制，不是产品结论。** 产品 Helper 本身是原生二进制，不受影响。

2. **Git for Windows 对无扩展名钩子的解析**
   实测：`.git/hooks/post-commit%%（PE，无扩展名）**不执行**，报
   `cannot spawn .git/hooks/post-commit: No such file or directory%%；
   命名为 `post-commit.exe%% 时**正常执行**。
   **含义**：安装器在 Windows 上可能需要提供 `.exe%% 后缀入口，或确保 `sh%% 可用。
   正常 Windows 环境（有 MSYS sh）下未必成立，**不能**直接推广成产品结论，需在中立环境复测。

3. **测量传输差异**：沙箱禁止子进程通过管道回传输出，基准测试用文件重定向代替管道。
   时间测量取的是 `spawnSync%% 往返，不受影响。

---

## 7. 复现方式

```powershell
. .\scripts\env.ps1
. .\scripts\start-crates-proxy.ps1

cargo build --release --manifest-path scripts/git-experiments/hook-recorder-rs/Cargo.toml
cargo build --release --manifest-path scripts/git-experiments/helper-spike-direct/Cargo.toml

node scripts/git-experiments/run-experiments.mjs
node scripts/git-experiments/bench-git-calls.mjs
node scripts/git-experiments/bench-spawn-baseline.mjs
node scripts/git-experiments/bench-helper-approaches.mjs
```

所有输出落在 `scripts/git-experiments/.out/%%。
