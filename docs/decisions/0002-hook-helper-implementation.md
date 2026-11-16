# ADR-0002：Git Hook Helper 的实现方案

- **状态**：**Accepted**
- **日期**：2026-11-16（第二次修订：三种方案现在**全部有实测数据**）
- **相关规范章节**：§9.5、§9.2、§44.1、§44.4、§30.2
- **证据**：`scripts/git-experiments/.out/bench-three-approaches.json`

---

## 修订说明（重要）

第一版本 ADR 声称「方案 B 无法构建，因此没有数据」。那是**环境缺陷**，不是产品结论。
该缺陷（GNU 工具链缺 `as.exe`）已修复，方案 B 现在可以构建并实测。
本版用**三方案真实对比数据**重写了结论——结论没有变，但依据更硬了。

---

## 背景

规范 §9.5 要求以 **Phase 0 实测 Hook 耗时**为依据，在三种 Helper 实现方案中选择：

- **A**：调用 `git` 命令
- **B**：链接 `gix` / `git2` 库
- **C**：直接读取 `.git` 文件

硬预算：

| 约束 | 值 | 出处 |
| --- | ---: | --- |
| Hook p95 执行时间目标 | < 50 ms | §44.1 |
| Hook 硬超时 | 200 ms | §9.2 |

---

## 实测结果（同一仓库、同一批事实、各 40 次样本）

```text
approach                          min      p50      p95      max   correct
A  单次 git 子进程             34.33    35.78    38.73    42.95    true
B  gix（进程内链接）           54.78    58.38    66.16   262.41    true
C  直读 .git（1 个原生进程）    9.76    10.91    13.40    72.09    true
```

三者对 HEAD 的判断**完全一致**（correct 全为 true），因此差异纯粹来自实现方式。

### 读数要点

- **A** 已吃掉 50 ms 预算的 **77%**，且这是「压成一次调用」的理论最优形态；真实 Helper 需要 reflog、git dir、commondir 等，一次拿不全。
- **B（gix）反而比 A 更慢**：66 ms p95。进程内没有子进程开销，但 gix 的仓库打开、对象解压与配置解析成本更高，而且二进制更大。
  **这是第一版 ADR 无法得知的事实**——当时它被记录为「未验证」。
- **C** 9.8–13.4 ms，且余量最大。

---

## 决策

**采用方案 C：直接读取 `.git` 的 Rust Helper（零外部 crate 依赖）。**
仅在 C 无法覆盖的场景（对象已在 packfile 中）回退到一次 `git` 调用。

---

## 备选方案

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| A　调 git 命令 | 语义最权威 | 单次 ~35 ms；17 次即 ~600 ms，**必然违反 200 ms 硬超时** | 仅作回退 |
| B　链接 gix | 语义权威 + 无子进程 | **实测 p95 66 ms，本身就超预算**；二进制显著变大 | 否 |
| C　直读 `.git` | 10.9 ms；零依赖；完全可控 | 需自行处理 packfile、SHA-256 | **采用** |

---

## 后果

### 正面

- Hook p95 从 **672 ms**（朴素外派）降到 **13.4 ms**，优于目标 3.7 倍。
- 零外部 crate 依赖，二进制小，不受工具链限制影响。
- 可在 Golden Tests 中用固定仓库目录直接验证解析结果。

### 负面 / 代价

- 需要自行实现并维护：zlib inflate、commit 解析、ref 解析、packed-refs。
  Phase 0 的 spike 已实现 loose object + 完整 inflate（stored / fixed / dynamic Huffman）。
- **packfile 尚未支持**：spike 只读 loose object。`git gc` / `git repack` 后对象进入 packfile，
  此时 C 会失败。**Phase 1 必须补齐 packfile + delta 解析，或在该场景回退到 A。**
- **SHA-256 仓库未支持**（Git 2.29+ 可选特性）。
- 自研解析的正确性风险高于调用 git，因此**必须有 Golden Tests 对照 `git` 输出**。

### 回退策略

| 场景 | 行为 |
| --- | --- |
| loose object 可读 | 走 C |
| 对象在 packfile | 回退到 1 次 `git log -1 --pretty=...`（约 35 ms，仍在预算内） |
| 任何解析失败 | 记录诊断 + 回退到 A；**不得**静默丢弃事件 |
| SHA-256 仓库 | 明确报错并降级，不得假装成功 |

---

## 已完成的正确性交叉验证

- `crates/git-hooks/tests/gitread_against_real_repo.rs`：对 HEAD sha、tree、author/committer、subject、
  时间戳、merge 父提交数、worktree、缺失对象，逐项与 `git` 输出对照（5 个测试）。
- `scripts/git-experiments/bench-three-approaches.mjs`：三方案输出的 head sha 一致。

---

## 复查计划

Phase 1 完成 packfile 支持后，用 Golden Tests 对照 `git` 输出复测本 ADR 的性能结论。
