# Phase 0 总结报告

- **日期**：2026-11-16
- **状态**：**Partially Implemented**（详见 [STATUS.md](../STATUS.md)）
- **检查点**：**未跨越任何检查点**。Phase 0 不是人工检查点，检查点 A 需 Phase 2a 完成。

---

## 1. 这一阶段到底做成了什么

### 1.1 工程基础（真实可用）

| 项 | 结果 |
| --- | --- |
| Monorepo（pnpm + Cargo workspace） | 12 个 crate + 1 个 app 可构建 |
| Rust 工具链 | 隔离安装（工作区内，可整体删除），rustc 1.100.0 |
| `cargo fmt / clippy / test / build%% | 全绿（38 个测试） |
| WXT + MV3 扩展 | 构建成功，产物含 manifest + wasm |
| 规范拆分 | 49 章 → 47 文件，覆盖率断言通过 |
| ADR | 6 份有实质结论 |

### 1.2 关键技术结论（有实测数据支撑）

**① Helper 实现方案：直读 `.git%%（ADR-0002）**

```text
方案                           p50       p95     判定
A  单次 git 调用              35.47 ms  38.34 ms  吃掉 50ms 预算的 77%
A' 11 次 git 调用            393.78 ms 428.26 ms  违反 200ms 硬超时
B  链接 gix/git2                  —         —     本机无法构建
C  直读 .git（1 个原生进程）  10.35 ms  12.53 ms  ✅ 达标
```

结论：**p95 从 428 ms 降到 12.5 ms**，从「必然超时」变成「优于目标 4 倍」。

**② 分类等待窗口：300 ms 不够（ADR-0004）**

实测 post-rewrite 在 Amend 时延后 **436 ms**、Rebase 时 **452 ms** 才到达，
默认 300 ms 窗口在本环境**必然超时**。已改为 **800 ms**（标 Provisional）。

**③ 一个反直觉发现**

**交互式 rebase 的 reword 完全不触发 post-rewrite。** 这意味着只靠 post-rewrite 识别重写，
会把 reword 误判成新提交。这是**已知局限**，必须写进用户文档，不能假装解决。

**④ WASM 可行（ADR-0001，部分）**

- Core 编译为 wasm32：**27,569 bytes**
- Node 中实例化：**0.606 ms**，**零宿主导入**
- 与 native 输出**逐字节相同**
- 浏览器内加载：**未验证**（沙箱禁止启动浏览器）

**⑤ 其他 Git 行为事实**

- post-commit 执行时 reflog **已含** HEAD（14/14 场景）
- `post-merge%% 的 argv 天然区分 squash（`0%% / `1%%）
- empty commit 的 tree 与父提交相同 → 可靠判定信号
- `core.hooksPath%%、worktree、sparse checkout 均不影响 Hook 触发

---

## 2. 没做成的，以及为什么

| 项 | 状态 | 真实原因 |
| --- | --- | --- |
| Tauri 2 运行 | **Blocked** | 无 MSVC；GNU target 不支持 Tauri Windows 打包 |
| SQLite + SQLx | **Blocked** | SQLx → windows-sys → dlltool → 缺 `as.exe%% |
| MV3 浏览器内 WASM | **Platform Unverified** | 沙箱禁止 Chromium 创建 mojo 命名管道 |
| Native Messaging 联调 | **未做** | 同上，需真实浏览器 |
| 最低 Git 版本 | **证据不足** | 只有 2.54.0 一个版本 |
| 桌面通知能力 | **未做** | Phase 0 无 Companion |
| 基础 CI 真实运行 | **未做** | 无 GitHub 仓库 |

**共同点**：都不是「没想到做」，而是**当前环境物理上做不到**。
每一条都在 STATUS 与 ADR 里写明了恢复条件。

---

## 3. 过程中被自己抓到的错误（全部写进文档，不隐藏）

| 错误 | 怎么发现的 | 教训 |
| --- | --- | --- |
| 直读 .git 第一版只支持固定 Huffman 块 | 基准脚本里的**正确性断言** | 只测时间不测结果会得到完全错误的结论 |
| 第二版错误地把范式码按位反转 | 同上，报 `invalid huffman code%% | — |
| stored-block 单元测试的字节是手写的且写错了 | 测试失败 | **测试向量必须来自独立实现**，最终改用 Node 的 zlib 生成 |
| `head_commit%% 返回的 sha 为空 | 交叉验证测试（对照 `git rev-parse HEAD%%） | 自研解析必须持续对照权威实现 |
| 交叉验证测试的 `assert_eq!%% 受参数求值顺序影响 | 失败信息显示 left 是 git 输出 | 不要把有副作用的调用写进断言参数 |
| `reflog_has_head%% 汇总用 `=== 1%% 比较布尔值 | 结果全是 false，与原始 trace 矛盾 | 汇总层出错会污染结论，必须回看原始数据 |
| 分类等待窗口延迟算成负数 | 取了「最后一个 post-commit」而非「紧邻的那个」 | 统计口径写错比代码错更难发现 |

清单本身就是证据：**这些错误如果不做交叉验证，全都会以「已完成」的形式交付出去。**

---

## 4. 环境适配（仅本机，不得进入产品）

| 问题 | workaround | 范围 |
| --- | --- | --- |
| schannel TLS 损坏 | git 用 `http.sslBackend=openssl%% | 进程级，不改用户全局配置 |
| cargo 无法连 crates.io | 本地 sparse registry 代理 | 写仓库内 `.cargo/config.toml%% |
| npm 缓存不可写 | 重定向到工作区 | 进程级 |
| PowerShell 5.1 + GBK | `.ps1%% 只用 ASCII | 脚本规范 |
| 管道式 stdio 被禁 | 子进程输出走文件重定向 | 实验工具 |
| MSYS `sh.exe%% 无法启动 | 探针改为原生 Rust PE | 实验工具 |
| pnpm 生命周期脚本被禁 | `--ignore-scripts%% + 手动 `wxt prepare%%%%，且需 `node-linker=hoisted%% | 构建 |

---

## 5. 需要你决定的三件事

**① 是否提供 MSVC 环境？**

影响面最大。一旦具备 VS Build Tools + Windows SDK：

- Tauri 2 可以真正构建运行；
- SQLx / gix 可以编译，Phase 1 的持久化工作可以开始；
- 「Blocked」项可以变绿。

**② 是否在沙箱外补做 MV3 浏览器验证？**

只需两步：打开 `chrome://extensions%% → 加载 `apps/extension/.output/chrome-mv3%% → 点开扩展图标。
popup 会显示 `WASM core: loaded 27569 bytes, instantiated in X ms`。看到这行就说明 ADR-0001 第 2 项通过。

**③ 是否创建 GitHub 仓库？**

CI 写好但从没跑过。仓库建好后可以：

- 首次真实运行 CI；
- 跑多 Git 版本矩阵，关闭 ADR-0003；
- 把 trace fixtures 纳入回归。

---

## 6. 下一步计划（Phase 1）

Phase 1 目标是「独立于 UI 的核心业务逻辑」。按依赖排序：

| 顺序 | 工作 | 前置 |
| --- | --- | --- |
| 1 | 事件模型、验证、来源可信度、去重键 | 无 |
| 2 | 仓库身份与用户身份识别 | 无 |
| 3 | 时区与日期策略实现 + 属性测试 | ADR-0005（已定选型） |
| 4 | 统计投影与 Engine（含读集） | 1–3 |
| 5 | 每日等级、永久成就（含 required_capabilities） | 4 |
| 6 | 用户过滤器 | 4 |
| 7 | Outbox 与通知所有权状态机（纯逻辑） | 1 |
| 8 | Golden Tests：回放 Phase 0 的 trace fixtures | 4 |
| 9 | SQLite Adapter | **需 MSVC** |

第 9 项与「用真实数据库验证 Engine」都卡在 MSVC 上，其余 8 项**可以现在就开工**。

---

## 7. 结论

Phase 0 交付的是：**一个可构建、可测试、有实测数据的工程底座，加上 5 份有结论的 ADR 和 15 份可回放的 Git 行为 fixture。**

它**没有**交付：可运行的桌面应用、可用的数据库、浏览器内验证过的 WASM。

这三项都必须在真实环境里补测，而不是在文档里宣布完成。
