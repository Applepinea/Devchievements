# 多 Git 版本实验矩阵

本文件是 [ADR-0003](../decisions/0003-minimum-git-version.md) 的证据附录。
ADR-0003 有四处在指向本文件，之前它不存在——一个悬空的证据链接比没有链接更糟。

---

## 1. 怎么跑出来的（可复现）

```powershell
# 三个 PortableGit 发行包解到 default-workspace\_gitver\<version>\
# 每个版本跑同一条实验脚本，PATH 与 TEMP 指到该版本与工作区内部
$env:PATH = "<dir>\mingw64\bin;<dir>\usr\bin;<dir>\cmd;" + $env:PATH
$env:TEMP = "<workspace>\_toolchain\tmp"
$env:TMP  = $env:TEMP
node scripts/git-experiments/run-experiments.mjs
```

原始输出（16–17 KB JSON，含每个场景的 hook 顺序、reflog 文本、耗时）：

```text
C:\Users\csa\Documents\deepseek-harness\default-workspace\_gitver\results2\2.30.2.json
C:\Users\csa\Documents\deepseek-harness\default-workspace\_gitver\results2\2.40.1.json
C:\Users\csa\Documents\deepseek-harness\default-workspace\_gitver\results2\2.54.0.json
```

> 这三个 JSON **不在仓库里**：它们在 workspace 层，不在 `devchievements/` 下。
> 所以「跨版本一致」是可复现的实验观察，不是仓库内可离线重放的资产。

### 环境

| 项 | 值 |
| --- | --- |
| 主机 | `win32 / x64 / 10.0.26300`（Windows 11） |
| 发行包 | git-for-windows 官方 PortableGit |
| 每版本次数 | **1 次** |
| 生成时间 | 2026-11-19T11:36:56Z – 11:37:58Z |
| 每版本场景数 | 15 / 15 |
| 脚本退出码 | 0 / 0 / 0 |

---

## 2. 版本矩阵

| Git 版本 | 结果 | 说明 |
| --- | --- | --- |
| 2.20.1.windows.1 | ❌ 无法运行 | 第 1 个场景即失败：`error: unknown switch 'b'`（无 `git init -b`） |
| 2.30.2.windows.1 | ✅ 15/15 | 实测通过的最低版本 |
| 2.40.1.windows.1 | ✅ 15/15 | |
| 2.54.0.windows.1 | ✅ 15/15 | 已提交 fixtures 的录制版本 |

---

## 3. 逐场景跨版本对照（本文件的核心）

把每个场景的 `order`（hook 触发顺序）、`hook_counts`、`total_events`、`reflog_all_true`、
`rewrite_kinds`、`branches`、`parents_shape`、`git_dir_kinds` 提出来做投影，**三个版本逐项比较**：

```
SAME 01-plain-commit
SAME 02-amend
SAME 03-amend-then-commit
SAME 04-rebase
SAME 05-interactive-rebase-reword
SAME 06-merge-no-ff
SAME 07-merge-ff
SAME 08-empty-commit
SAME 09-cherry-pick
SAME 10-squash
SAME 11-worktree
SAME 12-custom-hooks-path
SAME 13-existing-hook-chaining
SAME 14-submodule
SAME 15-sparse-checkout
```

**15/15 场景在三个版本上投影完全相同。** 复现方法：读上面三个 JSON，按 `scenario` 分组，
对上述字段取 `Sort-Object -Unique`，每组只剩一个值即为 SAME。

`reflog_messages`（人类可读的 reflog 文本）也逐字一致：15 个场景全部 `identical`，0 个 `differ`。

### 唯一一处纯文本差异

`06-merge-no-ff` 的输出在 2.30.2 上是 `Merge made by the 'recursive' strategy.`，
在 2.40.1 / 2.54.0 上是 `'ort'`。
该场景的 hook 顺序、事件数与 reflog 三版本完全一致，因此**不构成阻断**；
但它是「不要解析 Git 的人类可读输出」的一个现场证据。

---

## 4. `post-rewrite` 延迟（ADR-0004 的输入）

| 版本 | 02-amend | 03-amend-then-commit | 04-rebase |
| --- | --- | --- | --- |
| 2.30.2 | 413.179 ms | 402.790 ms | 528.582 ms |
| 2.40.1 | 388.550 ms | 466.195 ms | 446.057 ms |
| 2.54.0 | 458.173 ms | 448.087 ms | 534.845 ms |

跨版本区间 **388.550 – 534.845 ms**（9 个样本）。
这独立复现了 ADR-0004 的核心结论：**300 ms 不够**，800 ms 有余量。

> 样本量：每格 1 次，共 9 次。**这不是抖动范围**，只是 9 个独立观测值；
> 不足以做统计结论，足以否定「300 ms 够用」。

---

## 5. 证据缺陷：交互式 reword 的旧结论是 harness 产物

初版 ADR-0003 / ADR-0004 与 `git-behavior-experiments.md` §2.4 都写着
「交互式 reword 上 `post-rewrite` 根本不触发」。本次复测证明**该结论是错的**。

### 5.1 根因（两个，都在实验脚本里）

1. **反斜杠路径喂给 MSYS 程序**。`scripts/git-experiments/run-experiments.mjs:270` 把
   `GIT_SEQUENCE_EDITOR` 设成 `C:\Users\...\seq-editor.sh`。Git for Windows 自带的是 MSYS 程序，
   反斜杠被当转义符吃掉，路径变成 `C:Userscsa...`，编辑器根本起不来，rebase 直接失败。
   于是「没有 `post-rewrite`」被记成了 Git 的版本差异。改成正斜杠路径后 rebase 正常执行。
2. **`sed -i` 写入不确定**。脚本用 `sed -i "s/^pick/reword/"` 改写 todo 列表；
   在 git-bash 环境下 6 次重复里只有 1 次真正改写成功，其余 5 次退出码为 0 但实际执行的是普通 rebase。

### 5.2 修复后的观察

用正斜杠路径 + 不依赖 `sed -i` 的探测方式重跑，三个版本行为一致：
reword **确实会**产生 `post-rewrite`，而且是**两次**（先 `amend`，再 `rebase`）。

### 5.3 对已提交资产的影响（**未修**）

`packages/golden-tests/fixtures/git-traces/05-interactive-rebase-reword.json`
是上述缺陷的产物：它记录的是「rebase 失败」的结果。

本次**不修改**该 fixture，也不修改依赖它的 ADR-0004 数值——
重录 fixtures 会同时改变 golden tests 的期望值，属于独立变更。

**在该 fixture 重录之前**，场景 05 的 golden test 证明的是「旧录制结果被稳定重放」，
**不是**「交互式 reword 的真实 Git 行为」。这是登记在案的技术债。

---

## 6. 这份证据没有证明什么

1. **CI 从未运行。** `.github/workflows/ci.yml` 的 `git-version-matrix` job 是配置，
   没有在 GitHub Actions 上执行过一次。
2. **2.20.1 与 2.30.2 之间的版本一个都没测。** `git init -b` 在 2.28 引入（依据发布说明，
   未逐版本验证），所以真实下限落在 (2.20.1, 2.30.2] 区间内，本文件不宣称该区间内任何具体版本可用。
3. **单主机、每版本单次运行。** 没有多机重复，也没有对耗时重复采样。
   行为结论（hook 顺序、事件数、reflog 文本）可信度高于耗时结论。
4. **只有 2.54.0 的 trace fixtures 进了仓库。**
5. **Windows 之外未测。** 按 ADR-0017 本项目只支持 Windows，因此不构成缺口。
