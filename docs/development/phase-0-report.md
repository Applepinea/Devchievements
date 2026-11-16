# Phase 0 总结报告

- **日期**：2026-11-16
- **状态**：**Partially Implemented**（详见 [STATUS.md](../STATUS.md)）
- **检查点**：**未跨越任何检查点。** Phase 0 本身不是人工检查点；检查点 A 需 Phase 2a 完成。

---

## 1. 一句话结论

交付了**一个可构建、可测试、有实测数据的工程底座**，加上 **7 份有结论的 ADR** 和 **15 份可回放的 Git 行为 fixture**。

它**没有**交付：可运行的桌面应用、浏览器内验证过的 WASM、真实浏览器里的扩展联调。
这三项都必须在中立环境补测，而不是在文档里宣布完成。

---

## 2. 一条命令验证一切

```powershell
node scripts/verify.mjs
```

```text
PASS  fmt                       174 ms
PASS  clippy                    822 ms
PASS  test                     2970 ms   42 tests passed, 0 failed
PASS  build                    3982 ms
PASS  build-wasm                396 ms
PASS  extension-build         10371 ms
ALL CHECKS PASSED
```

---

## 3. 关键技术结论（全部有实测数据）

### ① Helper 实现方案：直读 `.git`（ADR-0002）

同仓库、同批事实、各 40 次样本：

```text
方案                          min      p50      p95      max   正确性
A  单次 git 子进程          34.33    35.78    38.73    42.95   ✅
B  gix（进程内链接）        54.78    58.38    66.16   262.41   ✅
C  直读 .git（1 个进程）     9.76    10.91    13.40    72.09   ✅
```

**最值得记录的一点**：进程内的 **gix 反而比子进程更慢**（66 vs 39 ms）。
这是第一轮**无法得知**的（当时 gix 连构建都不行）。如果按「库一定比子进程快」的直觉选型，会选错。

### ② 分类等待窗口：300 ms 不够（ADR-0004）

实测 post-rewrite 延迟：Amend **436 ms**、Rebase **452 ms**。规范默认 300 ms **必然超时**。已改 800 ms。

### ③ 一个无法靠 hook 解决的局限

**交互式 rebase 的 reword 不触发 post-rewrite。** 只靠 post-rewrite 识别重写，会把 reword 误判成新提交。
这条写进了用户文档与风险分析，**没有假装解决**。

### ④ WASM 可行（ADR-0001，部分）

- 产物 **27,569 bytes**，实例化 **0.606 ms**，**零宿主导入**；
- 与 native 输出 **逐字节相同**；
- **浏览器内加载未验证**（沙箱禁止启动浏览器）。

### ⑤ SQLite 可用（本轮解除阻塞）

5 个真实数据库测试：往返、幂等重复拒绝、事务回滚无残留、外键级联、关闭重开数据保留。
**这不是 mock，是真的 SQLite 文件。**

---

## 4. 本轮的两次「解除阻塞」值得单独说明

第一轮结束时 SQLx 和 gix 都标成 `Blocked`，原因是缺 `as.exe`。

当时如果就此停手，会留下两个错误印象：
「SQLx 在这个环境用不了」「gix 没有数据」。**两者都是错误的。**

实际补齐过程（逐层暴露、逐层修复）：

```text
缺 as.exe            -> 从 MSYS2 binutils 包提取
as.exe 缺 DLL       -> libintl / zlib / zstd / libiconv（走 PE 导入表自动解析）
缺 gcc.exe           -> 建别名指向 mingw gcc
gcc 缺 cc1           -> 装完整 mingw-w64 GCC 14.2.0
cc1 缺 DLL           -> gmp / mpfr / mpc / isl / gcc-libs
缺 C 头文件          -> headers 包
缺 CRT / 导入库      -> crt 包
缺 libpthread        -> winpthreads 包
缺 ar                -> binutils 全套
缺 default-manifest.o -> windows-default-manifest 包
```

全部固化进 `scripts/install-toolchain.ps1`（幂等），不再依赖口口相传。

---

## 5. 没做成的，以及为什么

| 项 | 状态 | 真实原因 |
| --- | --- | --- |
| Tauri 2 运行 | **Blocked** | 无 MSVC；GNU 目标不支持 Tauri Windows 打包 |
| MV3 浏览器内 WASM | **Platform Unverified** | 沙箱禁止 Chromium 创建 mojo 命名管道 |
| Native Messaging 联调 | **未做** | 同上 |
| 最低 Git 版本 | **证据不足** | 只有 2.54.0 一个版本 |
| 桌面通知能力 | **未做** | Phase 0 无 Companion |
| 基础 CI 真实运行 | **未做** | 无 GitHub 仓库 |

**共同点**：不是「没想到做」，而是**当前环境物理上做不到**。每条都在 STATUS 与 ADR 写明恢复条件。

---

## 6. 被自己抓到的错误（全部留在文档里）

| 错误 | 怎么发现的 | 教训 |
| --- | --- | --- |
| 直读 .git 第一版只支持固定 Huffman 块 | 基准脚本里的**正确性断言** | 只测时间不测结果会得出完全错误的结论 |
| 第二版错误地把范式码按位反转 | 同上，报 `invalid huffman code` | — |
| stored-block 单元测试的字节是手写且写错的 | 测试失败 | **测试向量必须来自独立实现**（改用 Node 的 zlib） |
| `head_commit` 返回的 sha 为空 | 与 `git rev-parse HEAD` 交叉验证 | 自研解析必须持续对照权威实现 |
| 交叉验证测试的 `assert_eq!` 受参数求值顺序影响 | 失败信息里 left 是 git 输出 | 别把有副作用的调用写进断言参数 |
| `reflog_has_head` 汇总用 `=== 1` 比较布尔值 | 结果全 false，与原始 trace 矛盾 | 汇总层出错会污染结论，必须回看原始数据 |
| 延迟算成负数 | 取了「最后一个 post-commit」而非「紧邻的那个」 | 统计口径错比代码错更难发现 |
| **`spawnSync({encoding:'utf8'})` 静默返回空串** | 依赖解析「成功」但文件没装上 | 沙箱下**绝不能用管道捕获子进程输出**，一律文件重定向 |
| PowerShell 反复吞掉 cargo 输出 | 多次拿到空日志 | 改用 `scripts/cargo-run.mjs` 固定环境与输出通道 |

**这张表本身就是证据**：这些错误如果不做交叉验证，全都会以「已完成」的形式交付出去。

---

## 7. 环境适配（仅本机，不得进入产品）

| 问题 | workaround | 范围 |
| --- | --- | --- |
| schannel TLS 损坏 | git 用 `http.sslBackend=openssl` | 进程级 |
| cargo 无法连 crates.io | 本地 sparse registry 代理 | 仓库内 `.cargo/config.toml` |
| 无 MSVC / 无 C 工具链 | 隔离的 mingw-w64 GCC + binutils | `_toolchain/` |
| npm 缓存不可写 | 重定向到工作区 | 进程级 |
| PowerShell 5.1 + GBK | `.ps1` 只写 ASCII | 脚本规范 |
| 管道 stdio 被禁 | 文件重定向 | 全部脚本 |
| MSYS `sh.exe` 无法启动 | 探针改为原生 Rust PE | 实验工具 |
| pnpm 生命周期脚本被禁 | `--ignore-scripts` + 手动 prepare；`node-linker=hoisted` | 构建 |

---

## 8. 需要你决定的三件事

### ① 是否提供 MSVC 环境？

现在只剩 Tauri 一项卡在这里。其余 Rust 侧（Core / Helper / WASM / SQLite / 测试）**全部已可用**。

### ② 是否在沙箱外补做 MV3 浏览器验证？

两步：打开 `chrome://extensions``` → 加载 `apps/extension/.output/chrome-mv3``` → 点开扩展图标。
popup 显示 `WASM core: loaded 27569 bytes` 即算通过。也可直接运行 `scripts/wasm-spike/verify-mv3-csp.mjs```。

### ③ 是否创建 GitHub 仓库？

CI 写好了但从没跑过。建好后可首次真实运行，并跑多 Git 版本矩阵关闭 ADR-0003。

---

## 9. 下一步（Phase 1）

| 顺序 | 工作 | 前置 |
| --- | --- | --- |
| 1 | 事件模型、验证、来源可信度、去重键 | 无 |
| 2 | 仓库身份与用户身份识别 | 无 |
| 3 | 时区与日期策略 + 属性测试 | ADR-0005（已定） |
| 4 | 统计投影与 Engine（含读集） | 1–3 |
| 5 | 每日等级、永久成就 | 4 |
| 6 | Outbox 与通知所有权状态机（纯逻辑） | 独立 |
| 7 | **packfile 解析** | 无（但**风险最高**，见 risk-analysis A3） |
| 8 | Golden Tests 回放 15 份 fixture | 4 |
| 9 | SQLite Adapter 扩展 | 4（环境已就绪） |

**第 7 项虽然排在后，却是唯一可能推翻 ADR-0002 的项**：真实仓库迟早会被 `git gc```，
而当前直读实现只支持 loose object。建议 Phase 1 优先评估。
