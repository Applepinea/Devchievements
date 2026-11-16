# 第 9 章　Git Hooks 与本地事件采集

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 9. Git Hooks 与本地事件采集

## 9.1 支持的 Hooks

首发：`post-commit`、`post-rewrite`、`post-merge`。

## 9.2 Hook 原则

- 不阻塞 Git 正常操作；设置硬超时（默认 200 ms，可配置）；失败不得影响 Commit。
- 不依赖 Node.js 或 Python，使用 Rust Helper。
- 支持并发 Hook 与离线事件持久化。

## 9.3 Git 环境兼容

必须考虑：Husky、Lefthook、自定义 `core.hooksPath`、`.git` 文件形式的 Worktree、多 Worktree、已存在的 Hook。

补充场景（Phase 0 实验，并在 ADR 中明确是否支持与降级策略）：Git Submodule（子模块视为独立仓库身份，其 Hook 位于父仓库的 modules 目录）；Sparse Checkout（至少验证 Hook 行为不受影响）。

## 9.4 安装与卸载

1. 安装前检测现有 Hook 并备份原始配置；不得覆盖用户已有 Hook。
2. 安装 Devchievements Hook，并链式执行原有 Hook；记录安装状态。
3. 任何修改用户 Git 配置的操作必须有清晰说明和撤销方式。
4. 卸载时优先恢复原始 Hook，删除 Devchievements 创建的配置，不得误删用户原有 Hook 或其他工具的配置。
5. 残留 Hook 发现 Helper 二进制不存在时：静默退出，返回码必须为 0，不得导致 Git 操作失败，不得产生持续错误提示。

**共享 hooksPath、版本升级与被修改的链式脚本**

- 共享 `core.hooksPath`（多个仓库共用同一目录）：安装前检测，并明确告知「会影响所有使用该目录的仓库」；不得未经确认修改共享目录中他人维护的 Hook；提供只对选定仓库生效的方式（由 Helper 按仓库白名单判断）。
- 版本升级：Devchievements 管理的 Hook 必须带版本标记；发现已有但版本不匹配的 Hook 时，先备份，只替换由 Devchievements 管理的部分；Hook 到 Helper 的调用协议带版本号，Helper 向后兼容旧版 Hook。
- 卸载时若用户手动修改过链式脚本：对比安装时记录的内容哈希，不一致则不得自动覆盖或删除，展示差异并让用户选择（保留、恢复备份、手动处理）；永远不删除用户修改过的内容。

## 9.5 Helper 实现要求

- 目标是尽量减少进程启动次数。直接读取 `.git` 文件、使用 gix / git2、调用 git 命令三种方案，必须以 Phase 0 实测的 Hook 耗时为依据评估，并写入 ADR。
- 采集：Commit SHA、仓库路径与身份、parents、Author / Committer 信息与时间、分支、Remote、HEAD Reflog 最近一条动作信息。

## 9.6 Amend 与 post-rewrite 时序

这是核心可靠性要求。Hook 触发顺序与 Reflog 可见性必须在 Phase 0 通过实验确认，不得假设所有 Git 版本行为一致。

Phase 0 必须测试：普通 Commit；`git commit --amend`；普通 Rebase；交互式 Rebase；Amend 后立即连续 Commit；Companion 在线；Companion 离线。

记录：HEAD Reflog 内容；post-commit 执行时 Reflog 是否已出现对应记录；Amend 的 Reflog message（特别验证 `commit (amend)` 是否出现）；post-rewrite 的旧 SHA 到新 SHA 映射；Hook 实际触发顺序；不同 Git 版本的差异。不得把某个固定 Reflog 字符串当成所有版本稳定可靠的保证。

事件分类策略：

1. post-commit 读取 HEAD Reflog 最近动作，尽可能当场识别 Amend、Cherry-pick、Rebase 等。
2. 能确定的事件立即分类，走正常路径，不受等待窗口影响。
3. 无法确定时标记 `pending_rewrite_classification`，先持久化，但暂不产生最终通知意图。
4. 分类等待窗口默认 300 ms（以 Phase 0 实验结果由 ADR 调整，可配置）。窗口内 post-rewrite 到达则按映射关联。
5. 窗口超时仍未收到 post-rewrite，按普通 Commit 处理。
6. 迟到的 post-rewrite 只修正统计投影并写入诊断日志，不得撤回已发出的通知。
7. 两个 Hook 无论谁先到达，最终统计结果必须一致（属性测试）。
8. 不得先发出不可撤销的错误成就通知再尝试撤回；分类不确定时采取保守策略。
9. 延迟目标只针对已分类事件，不含等待窗口。

## 9.7 Git 版本策略与 Hook Trace Fixtures

- Phase 0 确定最低支持的 Git 版本并写入文档。
- Hook 行为实验在 CI 中跨多个 Git 版本运行。
- 实验结果录成可重放的 Hook 事件序列（hook 名称、参数、stdin、关键环境、单调时间戳、仓库状态、Reflog 尾部），存放于 `packages/golden-tests/fixtures/git-traces/`，由 Phase 1 的 Golden Tests 回放。

* 最低 Git 版本的决策标准：必须同时满足 ① 支持 `post-rewrite`；② Amend / Rebase 相关的 Reflog 行为经实验稳定；③ 支持 Worktree 与 `core.hooksPath`；④ 在目标平台主流系统或包管理器提供的版本范围内可用。取满足全部条件的最低版本，写入 ADR，并在安装向导中检测与提示。
* Phase 0 定义 trace 的 JSON Schema（`packages/golden-tests/fixtures/git-traces/schema.json`）。任何修改 Hook 行为或 Helper 采集逻辑的变更，都必须新增或更新对应 trace。

---
