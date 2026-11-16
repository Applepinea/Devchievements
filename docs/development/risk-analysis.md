# 技术风险分析（Phase 0 §48.3）

- **日期**：2026-11-16
- **状态**：随环境变化更新；当前版本反映**补齐工具链之后**的实际情况

> 规范 §48 第 3 步要求「分析规范中可能存在的技术风险」。
> 本文件不是风险清单的堆砌，而是**每条风险配上当前实测状态与应对**。
>
> 图例：🔴 未解决且会阻塞 · 🟡 部分缓解 · 🟢 已验证可接受 · ⚪ 环境限制（非产品风险）

---

## A. 架构层风险（规范自身的设计难点）

### A1 🟡 Amend / Rebase 分类时机

**风险**：post-commit 触发时无从知道这次提交是否会被 rewrite；依赖 post-rewrite 则要等待，且等待窗口与延迟预算冲突。

**实测**：post-rewrite 在 Amend 时延后 **436 ms**、Rebase 时 **452 ms**（Git 2.54.0）。
规范默认窗口 300 ms **必然超时**。

**更麻烦的**：**交互式 rebase 的 reword 根本不触发 post-rewrite**，因此无法靠 hook 区分「reword」与「新提交」。

**应对**：窗口改 800 ms（ADR-0004）；post-commit 阶段用 reflog/tree/parent 数做初步分类；
把交互式 reword 的局限**写进用户文档**，不假装解决。

### A2 🟡 Commit 去重的固有极限

**风险**：Rebase / Amend / Cherry-pick / Squash 都会产生新 SHA。规范明确「不能承诺跨所有 Git 操作绝对去重」。

**实测**：cherry-pick 后 patch 内容与源提交完全一致；empty commit 的 tree 与父提交相同。

**应对**：主去重键用 `canonical_repository_id + commit_sha%%；patch-id 仅作辅助；
识别到 rewrite 时**关联**而非简单丢弃；明确记录误判风险。

### A3 🔴 packfile 尚未支持（阻碍 Phase 1）

**风险**：ADR-0002 选定直读 `.git%%%，但 Phase 0 的实现**只读 loose object**。
`git gc%% / `git repack%% 之后对象进入 packfile（且常带 delta 链），当前实现会失败。

**影响**：真实的长期使用仓库**必然**被 gc，因此这不只是理论风险。

**应对**：Phase 1 **必须**补齐 packfile + delta 解析，或在该场景回退到一次 `git%% 调用（约 35 ms，仍在预算内）。
**任何情况下都不得静默丢弃事件。**

### A4 🟡 Engine 纯度的可持续性

**风险**：一旦有人在 Engine 里调 `SystemTime::now()%% 或随机数，Golden Tests 立刻失去确定性且很难察觉。

**应对**：`crates/core%% 已通过注入的 `ClockProvider%% / `TimezoneProvider%% / `IdGenerator%% 隔离外部依赖；
workspace lints 禁止 `unwrap/expect/panic%%；模块边界写进 AGENTS.md §6。
**尚未有自动检查**（如禁止 `std::time%% 的 clippy 规则），Phase 1 补。

### A5 🟡 通知「最多展示一次」无法在分布式意义上保证

**风险**：外部副作用（弹窗、声音）本质不可事务化，§16.1 也承认「不能保证严格 exactly-once」。

**应对**：幂等 ID + 状态机 + Offer/Claim/ACK；规范给出的原则是**宁可少通知，不重复通知**，
并用图鉴的新解锁标记兜底（`delivery_unknown%% 也可发现）。
Phase 0 未实现该状态机（Phase 1 纯逻辑部分）。

### A6 🟡 每日等级与永久成就的口径必须分离

**风险**：内置成就用固定官方口径，用户过滤器只影响普通统计。混用会导致「用户改了阈值，内置成就判定跟着变」——规范明确禁止。

**应对**：数据模型分开存（`unlocked_achievements%% / `daily_milestones%%）；
成就规则带 `rule_version%% 与 `statistics_profile%%；UI 必须标注「固定官方口径」与「用户过滤口径」。

### A7 🟡 时区「重放不变」的实现难度

**风险**：系统时区库会变，历史重算结果可能漂移。

**应对**：TZDB 随应用打包（ADR-0005）；`local_date%% 入库即固定；改时区必须询问用户是否重算。
**未做**：DST 边界与属性测试（Phase 1）。

---

## B. 实测暴露的实现风险

### B1 🟢 Hook 性能

**曾担心**：外派 git 会拖慢提交。
**实测**：朴素外派 p95 **428 ms**（违反硬超时）；直读 `.git%% p95 **13.4 ms**。
**结论**：风险已被架构选择消除，且这个选择是**被数据推着做出来的**，不是拍脑袋。

### B2 🟢 gix 并不更快

**曾假设**：进程内库一定比子进程快。
**实测**：gix p95 **66 ms**，比单次 git 子进程（38.7 ms）还慢。
**教训**：这类「看起来显然」的假设必须实测。

### B3 🟡 自研解析的正确性

**风险**：自己解析 git 对象格式，错了会静默丢数据。
**已做**：zlib inflate 的测试向量**取自独立实现**（Node 的 zlib），不是手写；
对照真实 git 输出的 5 个交叉验证测试。
**过程记录**：inflate 前两版都是错的，都是被**正确性断言**抓到的（只测时间的话会全部漏过）。

### B4 🟡 浏览器侧 WASM 未验证

**风险**：MV3 的 CSP 可能拒绝 `WebAssembly.instantiate%%；未验证前不能声称可行。
**现状**：manifest 已含 `wasm-unsafe-eval%% 且 wasm 已打包，但**从未在浏览器里跑过**。
**应对**：ADR-0001 保持 Partially Accepted；提供一条可在沙箱外一键执行的验证脚本。

---

## C. 环境风险（⚪ 非产品风险，但会污染结论）

| 风险 | 状态 | 应对 |
| --- | --- | --- |
| schannel TLS 损坏 | 🟡 已绕过 | git 用 openssl 后端；cargo 用本地 sparse 代理。**不得进入产品代码** |
| 无 MSVC | 🔴 未解决 | Tauri 无法构建；记为 Blocked，需用户提供环境 |
| 沙箱禁止浏览器启动 | 🔴 未解决 | MV3 内 WASM 无法验证 |
| 沙箱禁止 `sh.exe%% | 🔴 未解决 | shell 钩子链式执行、`git submodule add%% 无法验证 |
| 沙箱禁止管道 stdio | 🟡 已绕过 | 全部改用文件重定向。**曾导致依赖解析静默失败一次** |
| PowerShell 5.1 + GBK | 🟡 已绕过 | `.ps1%% 只写 ASCII |
| 无 GitHub 仓库 | 🔴 未解决 | CI 从未运行；最低 Git 版本无法定论 |

---

## D. 规范内部的不确定点（需要用户决定，不能由 Agent 自行拍板）

| # | 问题 | 为什么不能自行决定 | 建议 |
| --- | --- | --- | --- |
| D1 | Bundle Identifier 的真实域名 | §1.4 明确 `yourdomain` 只是占位符，**必须用户确认** | 发布前确认 |
| D2 | 商标与名称检索 | §1.4「快速同名搜索不能代替正式商标检索」 | 发布前由用户完成 |
| D3 | 最低 Git 版本 | 只有一个版本的数据；编造会违反 §44.2 | 需 CI 多版本矩阵 |
| D4 | 是否接受 GNU 目标的 Tauri 限制 | 影响整个桌面端可行性 | 见 Phase 0 报告「需要你决定的三件事」 |
| D5 | 许可证 | 当前 `UNLICENSED%% | 公开发布前确认 |

---

## 建议的优先级

1. **packfile 支持**（A3）—— 直接决定直读方案能否用于真实仓库。
2. **MSVC 环境**（C）—— 一次性解锁 Tauri 与全部 Windows 打包验证。
3. **浏览器验证**（B4）—— 决定 WASM 路线是否成立，成本极低（一条命令）。
4. **CI 与多 Git 版本**（D3）—— 关闭 ADR-0003。

其余（A1/A2/A4–A7）在 Phase 1 按规范顺序处理。
