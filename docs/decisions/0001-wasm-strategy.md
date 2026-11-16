# ADR-0001：WASM 方案

- **状态**：**Partially Accepted（编译与一致性已验证；MV3 浏览器内加载未验证）**
- **日期**：2026-11-16
- **相关规范章节**：§3.3、§30.2、§31.3、§36.1
- **证据**：`scripts/wasm-spike/.out/%% 与 [git-behavior-experiments.md](../development/git-behavior-experiments.md)

---

## 背景

规范 §3.3 要求 Phase 0 做独立 WASM spike，验证 6 项：

1. Rust Core 能否编译为 WASM；
2. 目标浏览器 MV3 环境能否加载（**含 CSP `wasm-unsafe-eval%% 的实际验证**）；
3. 序列化行为是否一致；
4. 时间、时区与日期逻辑是否一致；
5. Golden Tests 能否在 Native 与 WASM 下得到相同结果；
6. 性能是否满足需求。

决策规则：成功 → Native 与 Browser 共用同一 Rust Core；失败 → Rust Native + TypeScript Browser 双实现，共用同一套 Golden Test Suite。

---

## 实测结果

| # | 验证项 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1 | 编译为 `wasm32-unknown-unknown%% | ✅ **通过** | `devchievements_core.wasm%% = **27,569 bytes** |
| 2 | MV3 浏览器内加载 + CSP | ❌ **本机无法验证** | 见「阻塞」一节，附完整错误日志 |
| 3 | 序列化一致性 | ✅ **通过** | native 与 wasm 输出**逐字节相同** |
| 4 | 时区/日期一致性 | ⏸ 未开始 | Phase 1 有时区实现后才能测 |
| 5 | Golden Tests 双跑 | ⏸ 未开始 | 需先有 Phase 1 规则 |
| 6 | 性能 | ✅ **通过** | 实例化 **0.606 ms**；单次调用远低于 5 ms 阈值 |

### 关键测量（`scripts/wasm-spike/.out/wasm-spike.json%%）

```text
wasm bytes      : 27569
instantiate ms  : 0.606
exports         : memory, devch_probe_len, devch_probe_write
checks          : instantiates_with_no_imports = true
                  exposes_probe_abi              = true
                  reports_nonzero_length         = true
                  written_length_matches         = true
                  native_matches_wasm            = true   <-- 关键
                  per_call_under_5ms             = true
```

**`native_matches_wasm = true%%** 的含义：同一个 `EngineOutput%% 在原生与 WASM 下序列化出的 JSON **完全相同**。这是「共用同一 Core」的前提，已成立。

**`instantiates_with_no_imports = true%%** 的含义：模块不依赖任何宿主函数，降低了 MV3 集成与 CSP 风险。

---

## 决策

**方向决定：继续走「Native 与 Browser 共用同一 Rust Core（WASM）」路线。**

理由（仅限于**已验证**的部分）：

- 产物仅 27.6 KB，远低于 2 MB 阈值；
- 实例化 0.6 ms，可忽略；
- 无宿主导入依赖；
- 与 native 输出逐字节一致。

**但这不是完整的「验收通过」**：第 2 项尚未验证。因此状态是 **Partially Accepted**，而非 Accepted。

---

## 阻塞：MV3 浏览器内验证在本机无法完成

**这不是产品问题，是沙箱限制。** 已尝试四种组合，全部失败：

| 尝试 | 结果 |
| --- | --- |
| Chrome 157 `--headless=new --load-extension%% + CDP | 进程崩溃，无 CDP 端口 |
| Chrome 157 有头模式 + `--load-extension%% | 同上 |
| Chrome + `--disable-crash-reporter --disable-breakpad --no-sandbox%% | 同上 |
| Microsoft Edge 157 同上参数 | 同上 |

实际错误：

```text
crashpad_client_win.cc:431 OpenProcess: Access denied (0x5)
crashpad_client_win.cc:145 crash server failed to launch, self-terminating
FATAL:mojo/public/cpp/platform/platform_channel.cc:112 Check failed: Access denied (0x5)
content/browser/network_sandbox.cc:472 Failed to grant sandbox access to cache directory: Access denied (0x5)
```

**根因**：沙箱禁止创建 Chromium mojo IPC 所需的命名管道。浏览器无法启动自己的子进程，因此既不能加载扩展，也不能暴露 CDP 端点。

完整记录见 `scripts/wasm-spike/.out/mv3-csp.json%%。

### 已经证明到什么程度

以下事实**已经证实**：

- 构建产物的 manifest **确实**包含 `script-src 'self' 'wasm-unsafe-eval'; object-src 'self'%%；
- `.wasm%% 资源**确实**被打进扩展包（`.output/chrome-mv3/wasm/devchievements_core.wasm%%，27,569 bytes）；
- 扩展**确实**能构建成功（WXT 0.20.27，MV3）。

**未证实**的只有一件事：真实页面在真实 CSP 下实例化 WASM 是否成功。

### 如何关闭这一项

1. 在沙箱外的机器上运行 `scripts/wasm-spike/verify-mv3-csp.mjs%%；或
2. 手动在 `chrome://extensions%% 加载 `.output/chrome-mv3%%，打开 popup 后看到「WASM core: loaded …」即算通过。

---

## 备选方案（仍然有效）

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 共用 Rust Core（WASM） | 单一实现、行为一致、体积小 | 浏览器内加载未验证 | **当前方向** |
| Rust Native + TypeScript Browser 双实现 | 无 wasm 风险 | 两份逻辑靠 Golden Tests 对齐，长期维护成本高 | 保留为失败回退 |

---

## 后果

- `crates/core%% 保持**零平台依赖**，可同时编译到 native 与 wasm32。
- 为支持 wasm 导出，`crates/core%% 使用 `crate-type = ["rlib", "cdylib"]%%，并在 `wasm_entry.rs%% 中有一处**受控的** `unsafe%%（指针拷贝）；crate 其余部分仍禁止 unsafe。
- 第 2、4、5 项完成前，**不得**在 STATUS 或对外文档中声称「浏览器侧已验证」。

---

## 复查计划

Phase 3a 开始前必须补齐第 2 项；Phase 1 完成时区实现后补齐第 4、5 项。补齐后把本 ADR 改为 Accepted 并附证据链接。
