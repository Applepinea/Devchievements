# ADR-0003：最低支持的 Git 版本

- **状态**：**Accepted（最低版本 = 2.30.x，证据边界见「证据边界」一节）**
- **日期**：2026-11-16
- **修订**：2026-11-19 由 `Proposed` 改为 `Accepted`。
  **改了什么**：补入 Git 2.30.2 / 2.40.1 / 2.54.0 三个真实版本的 15 场景实测
  （外加 2.20.1 的失败证据），并在 `.github/workflows/ci.yml` 增加 `git-version-matrix` job。
  **为什么改**：初版唯一的阻塞理由是「只有 2.54.0 一个版本的数据」，该缺口现已补齐。
  **新证据**：[git-version-matrix.md](../development/git-version-matrix.md)。
- **相关规范章节**：§9.7、§30.3
- **证据**：[git-behavior-experiments.md](../development/git-behavior-experiments.md)、
  [git-version-matrix.md](../development/git-version-matrix.md)

---

## 背景

规范 §9.7 给出的最低版本判定标准，必须**同时**满足：

1. 支持 `post-rewrite`；
2. Amend / Rebase 相关的 Reflog 行为经实验稳定；
3. 支持 Worktree 与 `core.hooksPath`；
4. 在目标平台主流系统或包管理器提供的版本范围内可用。

并要求「在 CI 中跨多个 Git 版本运行实验」。

初版（2026-11-16）只有 `git version 2.54.0.windows.1` 一个版本的数据，四项标准里第 ② 项
在单版本、单平台下无法成立，故状态为 `Proposed` 且明确拒绝给出任何版本号。

---

## 实测数据

### 测试矩阵

全部使用 git-for-windows 官方 PortableGit 发行包，解包后把该版本的 `cmd`、`mingw64/bin`、
`usr/bin` 放到 `PATH` 最前，跑同一份 `scripts/git-experiments/run-experiments.mjs`：

| Git 版本 | 结果 | 说明 |
| --- | --- | --- |
| 2.20.1.windows.1 | ❌ 无法运行 | 第 1 个场景即失败：`error: unknown switch 'b'`，该版本没有 `git init -b` |
| 2.30.2.windows.1 | ✅ 15/15 场景完成 | 最低的实测通过版本 |
| 2.40.1.windows.1 | ✅ 15/15 场景完成 | |
| 2.54.0.windows.1 | ✅ 15/15 场景完成 | 已提交 fixtures 的录制版本 |

主机：`win32/x64/10.0.26300`（Windows 11），每个版本各跑一次，2026-11-19。

### 逐条对照 §9.7 的四项标准

| 标准 | 2.30.2 | 2.40.1 | 2.54.0 |
| --- | --- | --- | --- |
| ① `post-rewrite` | ✅ amend / rebase 均触发 | ✅ 同左 | ✅ 同左 |
| ② Reflog 行为稳定 | ✅ 15 场景的 reflog 消息文本逐字一致 | ✅ 同左 | ✅ 同左 |
| ③ Worktree + `core.hooksPath` | ✅ 场景 11、12 均通过 | ✅ 同左 | ✅ 同左 |
| ④ 主流系统/包管理器可获得 | ✅ 官方 PortableGit 安装包 | ✅ 同左 | ✅ 同左 |

标准 ② 的判定方式：把每个场景的 `hook_counts`、hook 触发顺序、`total_events`、
`reflog_all_true`、`rewrite_kinds`、`git_dir_kinds`、`parents_shape`、`branches` 提出来做投影，
三个版本的投影**逐字节相同**；唯一会变的是 `rewrite_mappings`（其中含提交 SHA）与
`rewrite_lag_ms`（计时）。逐场景对照表见 [git-version-matrix.md](../development/git-version-matrix.md) §3。

唯一一处纯文本差异：`06-merge-no-ff` 的输出在 2.30.2 是 `Merge made by the 'recursive' strategy.`，
在 2.40.1 / 2.54.0 是 `'ort'`。merge strategy 改名不影响 Hook 行为（该场景的事件数与顺序三版本一致），
因此不构成阻断；但它提醒「不要解析 Git 的人类可读输出」。

### `post-rewrite` 延迟（ADR-0004 的输入）

| 版本 | 02-amend | 03-amend-then-commit | 04-rebase |
| --- | --- | --- | --- |
| 2.30.2 | 413.2 ms | 402.8 ms | 528.6 ms |
| 2.40.1 | 388.6 ms | 466.2 ms | 446.1 ms |
| 2.54.0 | 458.2 ms | 448.1 ms | 534.8 ms |

跨版本区间 **388.6–534.8 ms**。这独立复现了 ADR-0004 的核心结论：**300 ms 不够**，
而 800 ms 在 amend / rebase 路径上仍有余量。

需要注意一处相反方向的数据：交互式 reword 场景（见下节）重建后在三个版本上都出现了
**第二次** `post-rewrite`（rebase 阶段）的延迟 **805–858 ms**，已到达 800 ms 窗口边缘。
该数字来自本次修复后的 harness（3 个版本各 1 次），样本太少，不足以推翻 800 ms；
但它说明 800 ms 对「reword 之后的 rebase 那一跳」余量很小，应在 Phase 1 的复查里重点复测。
ADR-0004 的数值本 ADR 不做修改（不在本次范围内），仅在此登记。

---

## 决策

1. **最低支持版本 = 2.30.x**。准确表述是「2.30.2 是实测通过的最低版本」；
   2.20.1 明确不可用（连实验都跑不起来）。
2. **安装向导必须检测 Git 版本并在低于 2.30 时提示**。
   注意：该检测**目前尚未实现**——`apps/desktop/src-tauri/src/preflight.rs` 只执行 `git --version`
   并显示版本字符串，不做任何版本比较；`installer-hooks.nsh` 只检查 WebView2。
   本条是待实现项，不是已交付功能。
3. **CI 必须跨版本跑实验**，固定 2.30.2 / 2.40.1 / 2.54.0 三个版本（`.github/workflows/ci.yml`
   的 `git-version-matrix` job）。不跟随 latest：跟随 latest 每月都会移动目标，且仍然说不出下限。
4. `STATUS.md` 的「最低 Git 版本」一项应从 `Not Started` 改为 `Implemented`
   （结论 + CI 配置已完成），CI 在 GitHub 上实际跑通之后才能是 `Tested`。
   `STATUS.md` 不由本 ADR 直接改写。

---

## 证据边界（必须与结论一起读）

以下都是**未验证**，任何文档不得把它们说成已验证：

1. **CI 矩阵从未在 GitHub Actions 上运行过。**
   `git-version-matrix` job 是本次新增的配置，落盘时只在本机用同一条断言脚本对三个版本的
   真实输出做过验证；仓库的 CI 至今没有一次成功运行（见 `STATUS.md` 的 CI 记录）。
   因此**不得**写「CI 验证通过」「CI 已覆盖多版本」。当前状态是「配置已实现，未实际执行」。
2. **2.20.1 与 2.30.2 之间没有任何版本被测过。**
   「最低 2.30.x」的真实含义是「2.30.2 是最低的实测通过版本」，**不是**「2.20.1 到 2.30.1 都不支持」。
   `git init -b` 在 2.28 引入（依据 Git 发布说明，未在本机逐版本验证），所以真实下限落在
   (2.20.1, 2.30.2] 区间内；本 ADR 不宣称该区间内任何具体版本可用或不可用。
3. **单主机、单次运行。** 三个版本都在同一台 Windows 11 主机上各跑一次，没有多机重复，
   也没有对耗时做重复采样取抖动范围。行为结论（Hook 顺序、事件数、reflog 文本）在跨版本上
   完全一致，可信度高于耗时结论；耗时结论应以 ADR-0004 的专项基准为准。
4. **只有 2.54.0 的 trace fixtures 进了仓库。** `packages/golden-tests/fixtures/git-traces/index.json`
   记录的 `git_version` 仍是 2.54.0；2.30.2 / 2.40.1 的运行结果只存在于实验输出目录，未提交。
   也就是说「跨版本一致」是本次实验的观察结果，不是仓库里可离线复现的资产。
5. **Windows 之外未测。** 按 ADR-0017 本项目只支持 Windows，因此不构成缺口；
   但若将来扩大平台范围，本标准必须重做。

---

## 证据缺陷：交互式 reword 场景的旧结论是 harness 产物

初版 ADR 与实验记录 §2.4 都写着「交互式 reword 在 2.54.0 上根本不触发 `post-rewrite`」。
本次复测证明**该结论是错的**，原因是实验脚本自身的两个缺陷：

1. `scripts/git-experiments/run-experiments.mjs` 把 `GIT_SEQUENCE_EDITOR` 设成一个**反斜杠路径**
   （`C:\Users\...\seq-editor.sh`）。Git for Windows 自带的是 MSYS 程序，反斜杠会被当作转义符吃掉，
   路径变成 `C:Userscsa...`，编辑器根本起不来，rebase 直接失败——于是「没有 `post-rewrite`」被记成了
   Git 的行为差异。改成正斜杠路径后，rebase 正常执行。
2. 该脚本用 `sed -i "s/^pick/reword/"` 改写 todo 列表。`sed -i` 在 git-bash 环境下的写入是
   **不确定的**：6 次重复里只有 1 次真正把 `pick` 改成了 `reword`，其余 5 次虽然退出码为 0，
   实际执行的是普通 rebase。

用正斜杠路径 + 不依赖 `sed -i` 的探测脚本在三个版本上重测，reword **确实会**产生 `post-rewrite`，
而且是**两次**：先 `amend`（reword 本身），再 `rebase`（后续提交重放）。三个版本行为一致。
细节与复现步骤见 [git-version-matrix.md](../development/git-version-matrix.md) §5。

对结论的影响：

- 决策 1–3 **不受影响**：三个版本都触发 `post-rewrite`，跨版本一致性反而更强。
- **已提交的 fixture `05-interactive-rebase-reword.json` 是上述缺陷的产物**：它记录的是
  「rebase 失败」的结果（`hook_order` 只有 3 个 `post-commit`，`rewrite_lag_ms` 为空）。
  本次**不修改**该 fixture，也不修改依赖它的 ADR-0004 数值——修复 harness 并重新录制
  fixtures 会同时改变 golden tests 的期望值，属于独立变更，已作为已知技术债登记。
- 在该 fixture 重录之前，场景 05 的 golden test 证明的是「旧录制结果被稳定重放」，
  **不是**「交互式 reword 的真实 Git 行为」。

---

## 备选方案

**A. 维持 `Proposed`，继续不给版本号。** 否决：跨版本证据已具备，再拖下去只会让安装向导
长期用「基于文档的保守下限」误伤可用版本，而 §9.7 要求的是实测结论。

**B. 用 2.1 作最低版本**（`post-rewrite` 的引入版本，即初版的临时保守下限）。否决：
`post-rewrite` 的存在不等于「amend 一定触发它」，也不等于 reflog 文本稳定；
而且 2.20.1 连 `git init -b` 都没有，2.1 的实际可用性无从谈起。

**C. 只支持最新版本（2.54+）。** 否决：把大量用户挡在门外，且与实测结论矛盾——
2.30.2 的 15 个场景与 2.54.0 逐字节一致。

**D. 要求 ≥ 2.40 以求更保险。** 否决：没有任何数据支持 2.30.2 与 2.40.1 之间存在行为差异；
凭空抬高门槛和凭空降低门槛一样是编造。

---

## 后果

- 安装向导的版本门槛从「基于文档的猜测」变成「有实测依据的 2.30」，
  但**该门槛本身还未实现**，见「决策」第 2 条。
- 2.20.1 及更早版本会被拒绝。这是有证据的拒绝（该版本无法执行本项目的 Hook 流程所需命令），
  不是保守估计。
- (2.20.1, 2.30.2) 区间内的版本被一并拒绝，属于**已知的证据空白**；若用户报告该区间有可用版本，
  应按「复查计划」补测而不是直接放宽门槛。
- CI 多了一个 windows-latest 矩阵（3 个 leg），每个 leg 要下载约 50 MB 的 PortableGit、
  编译 recorder 并跑 15 个场景；这是目前 CI 里最慢的 job，可接受。
- ADR-0004 的「数值为 Provisional」标注现在可以由跨版本数据收紧（388.6–534.8 ms 复现了
  「300 ms 不够」），但本 ADR 不改它；另外 reword 重建后出现的 805–858 ms 需要单独复查。

---

## 复查计划

1. CI 首次真正跑通后，把「证据边界」第 1 条从「未实际执行」改为实际结论，并据此决定
   `STATUS.md` 是否可升为 `Tested`。
2. 若要缩小下限的证据空白，按 2.24 / 2.26 / 2.28 补测；重点看 `git init -b` 与 `post-rewrite`。
3. harness 修复（正斜杠 `GIT_SEQUENCE_EDITOR`、去掉对 `sed -i` 的依赖）后重录场景 05 的 fixture，
   并同步修正 ADR-0004 与 `git-behavior-experiments.md` §2.4 的表述。
4. reword 路径 `post-rewrite` 的 805–858 ms 用重复采样复测，再决定 ADR-0004 的 800 ms 是否要上调。
5. 安装向导实现版本比较后，本条决策才真正闭环。
