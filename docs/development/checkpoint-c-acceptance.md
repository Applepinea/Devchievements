# 检查点 C 验收材料

> 生成日期：2026-11-17　｜　分支 `main`　｜　提交 `82bbcdb` 及之后
>
> 本文只写**实测过**的东西。每一条都能用给出的命令复现；
> 没验证的部分集中列在 §6，不混在成果里。

---

## 1. 三十秒理解这个产品

Devchievements 把开发活动变成成就。两种使用方式：

| 模式 | 做什么 |
| --- | --- |
| **Desktop Only** | Git 钩子捕获本地提交 → 本地 Rust 引擎判定 → 桌面弹窗 |
| **Full Experience** | 上面的全部，**加上**浏览器扩展检测 GitHub 上的提交/发布/PR/Issue |

**判定规则只有一份**，在 Rust 里。扩展只做检测，不做判定。
这是刻意的取舍（ADR-0018）：两套规则必然会分叉，而分叉后的错误数字比功能缺失更难发现。

---

## 2. 现在能演示什么

### 2.1 桌面端

```
真实 git commit → post-commit 钩子 → helper → Spool
                                      ↓
                    Companion 摄取 → SQLite → 引擎判定
                                      ↓
                              成就解锁 → 弹窗 + 音效
```

六个页面全部从 SQLite 读真实数据：Dashboard、成就图鉴、活动、仓库、设置、关于。

**一条命令看到全部：**

```powershell
node scripts/e2e/full-loop.mjs
```

### 2.2 浏览器端（Full Experience）

```
GitHub 页面 → 扩展检测 → Native Messaging → 本地 host 校验
                                              ↓
                              同一套判定引擎 → 同一个弹窗
```

**一条命令看到全部：**

```powershell
node scripts/e2e/browser-to-popup.mjs
```

这条脚本会把「浏览器的字节」一路送到桌面弹窗，中间每一环都断言。


---

## 3. 验收怎么跑

### 3.1 一次性检查（约 1 分钟）

```powershell
node scripts/verify.mjs
```

九步：格式、静态检查、单元测试、构建、WASM 构建、桌面 UI 构建、桌面构建、扩展构建、扩展测试。

**实际输出（本轮）**：

```text
PASS  fmt                       282 ms
PASS  clippy                   1459 ms  finished
PASS  test                     8900 ms  finished
PASS  build                     393 ms  finished
PASS  build-wasm                164 ms  finished
PASS  desktop-ui-build          279 ms
PASS  desktop-build            9251 ms  finished
PASS  extension-build           925 ms  finished
PASS  extension-test            166 ms

ALL CHECKS PASSED
```

### 3.2 端到端（22 个脚本，约 15 分钟）

```powershell
$scripts = Get-ChildItem scripts/e2e -Filter *.mjs
foreach ($s in $scripts) {
  node $s.FullName *> $null
  "{0}={1}" -f $s.BaseName, $LASTEXITCODE
}
```

**退出码含义**：0 = 已验证通过　1 = 失败　2 = 环境阻止验证（非产品缺陷）

**本轮实测结果**：

```text
hook-to-spool=0            full-loop=0          ipc-fallback=0
crash-recovery=0           desktop-tray=0       window=0
pages=0                    wizard=0             popup=0
notification-settings=0    upgrade=0            backup-restore=0
uninstall=0                diagnostics=0        privacy-mode=0
history-import=0           native-host=0        browser-to-popup=0
login-mapping=0            extension-load=0     service-worker-restart=0
toast=2
```

**21 个通过，1 个为 2。** `toast=2` 的原因见 §6.2，不影响任何功能。

---

## 4. 逐项对照验收标准

### 4.1 Phase 3a 验收（§36.2）

| 标准 | 状态 | 证据 |
| --- | --- | --- |
| 扩展可独立安装 | ✅ | `extension-load`：CDP `Extensions.loadUnpacked` 装进真实浏览器 |
| GitHub 事件识别与验证 | ✅ | Commit / Release / PR / Issue 四类；页面无机器可读日期时**忽略而非猜测** |
| IndexedDB 持久化正确 | ✅ | `service-worker-restart`：真实停掉 worker 后数据仍在 |
| 成就解锁与通知 | ⚠️ | 解锁在桌面端完成（ADR-0018）；浏览器内不解锁 |
| Service Worker 回收后恢复 | ✅ | 同上，含「worker 确实已停」的断言 |
| Golden Tests 通过 | ✅ | `cargo test --workspace` 全绿（382 项） |
| 不依赖 Native Messaging | ❌ **已不适用** | 产品决定改为扩展检测 + 桌面判定（ADR-0018） |

最后一条是**经授权的偏离**，不是遗漏。原规范假设浏览器能独立判定，
而那需要第二套规则或把引擎编成 WASM 并暴露完整 ABI。产品负责人选择了第三条路。

### 4.2 Phase 3b（Full Experience）

| # | 项 | 状态 |
| --- | --- | --- |
| 3b.1 | Native Messaging 协议 | ✅ 真实管道、真实分帧 |
| 3b.2 | 浏览器报告 → 本地判定 → 入库 | ✅ 11 项 e2e |
| 3b.3 | 扩展转发 + 本地留痕 | ✅ 18 项 e2e，含真实 `connectNative` 握手 |
| 3b.4 | Host 注册 + 固定扩展 ID | ✅ ID 由提交的公钥推导 |
| 3b.5 | 桌面弹窗呈现浏览器事件 | ✅ 6 项 e2e |
| 3b.6 | 注册与清理脚本 | ✅ 只动 HKCU |
| 3b.7 | PR / Issue 识别 | ✅ 14 项扩展测试 |


---

## 5. 关键技术决定（以及为什么）

验收时最容易被问的是「为什么这么做」，这里集中回答。

| # | 决定 | 原因 |
| --- | --- | --- |
| ADR-0001 | 桌面与浏览器共用同一份 Rust 核心 | 两套规则必然分叉，分叉的错误数字比功能缺失更难发现 |
| ADR-0015 | **不用系统通知，改用自有弹窗** | Windows 拒绝本程序发出的**所有** Toast，含 Explorer 的系统 AUMID；而 PowerShell 用同一 AUMID 能成功 → 失败在进程层，无法靠注册解决 |
| ADR-0017 | **只支持 Windows** | 产品决定。CI 只跑 `windows-latest` |
| ADR-0018 | **扩展只检测，桌面判定** | 第三条路：不让浏览器判定，避免第二套规则 |

### 5.1 系统通知为什么放弃

这是本轮最反直觉的决定，值得单独说：

```text
我们的 AUMID        → 0x80073D54
Explorer 的系统 AUMID → 0x80073D54   ← 决定性证据
PowerShell 用我们的 AUMID → 成功
```

如果问题在注册，系统自带标识不可能也失败。所以失败在**进程**。

产品自己的弹窗能画、可控、可测，于是通知渠道改成弹窗，
系统通知从降级链里**移除**（不是禁用，是不再尝试）。

---

## 6. 明确没有做到的部分

> 这一节比前面几节更重要。

### 6.1 只支持 Windows

macOS / Linux 未验证，已按 ADR-0017 标为**不支持（产品决定）**。
CI 只跑 Windows。其他平台的 `cfg` 分支保留但不维护——
删掉只会让非 Windows 构建以更晦涩的方式失败。

### 6.2 `toast=2` 的含义

退出码 2 = **环境阻止验证**，不是失败。
该通道已按 ADR-0015 弃用，所以这条脚本保持红色是**诚实的记录**，不是待修项。

### 6.3 扩展不能脱离桌面独立使用

没有桌面应用时，扩展会把检测结果存在本地，**但不解锁任何成就**。
这是 ADR-0018 的自觉选择。代价写在 STATUS 里，没有假装满足原验收标准。

### 6.4 浏览器内没有设置页和图鉴

浏览器内只有 Dashboard（读 IndexedDB）。登录名映射、图鉴等仍在桌面端。

### 6.5 成就「需要扩展」标记未实现

数据模型里这个字段一直空着。现在四类 GitHub 活动都支持了，
但还没有哪条成就规则依赖扩展能力，所以这个字段暂时没有真实用途。

### 6.6 至少两处已知的设计风险

**1. Native host 重塑 payload 时曾两次漏字段**（`author_login`、`merged`）。
两次都是「数据看起来完全正常，只是分类或归属全错」。
理想做法是整条透传、只在边界校验。已记为后续改进，**尚未改**。

**2. 多版本 Git 行为矩阵未实现**。
原 CI 里那个 job 从不真的安装不同 Git 版本，只打印一句警告。
我删掉了它——**打印警告不是覆盖率**。这项仍是空白。

---

## 7. 已知问题清单

| 严重度 | 问题 | 影响 |
| --- | --- | --- |
| 中 | host 重建 payload，漏字段风险 | 已发生两次；当前有测试守住已知字段 |
| 中 | 多版本 Git 未验证 | 只在 Git 2.54 上测过 |
| 低 | 浏览器内无设置页 | 映射需回桌面配置 |
| 低 | 「需要扩展」标记空置 | 暂无成就使用 |
| 记录 | 系统通知不可用 | 已改用自有弹窗，不影响用户 |


---

## 8. 代码规模（实测）

| 项 | 数字 |
| --- | --- |
| 提交数 | 52 |
| 跟踪文件 | 332 |
| Rust crate | 16 |
| 应用 | 5（desktop / helper / native-host / companion-headless / extension） |
| 单元测试 | **382 项**，40 个测试套件 |
| 端到端脚本 | **22 个** |
| 架构决策记录 | 14 份（ADR-0000 ～ 0018） |

---

## 9. 验收建议顺序

如果时间有限，按这个顺序看（约 20 分钟）：

| 顺序 | 做什么 | 看什么 |
| --- | --- | --- |
| 1 | `node scripts/verify.mjs` | 九步是否全绿 |
| 2 | `node scripts/e2e/full-loop.mjs` | 一次真实提交如何走完全程 |
| 3 | `node scripts/e2e/browser-to-popup.mjs` | 浏览器字节如何变成桌面弹窗 |
| 4 | `node scripts/e2e/pages.mjs` | 六个页面各自渲染 |
| 5 | 打开 `STATUS.md` | 逐项状态与未验证标注 |
| 6 | 看 §6 | 确认没有把未验证写成已验证 |

---

## 10. 一句话总结

**两条完整链路都通了，并且有端到端证据。**

- 桌面：真实 git commit → 真实弹窗，六个页面读真实数据库。
- 浏览器：真实 GitHub 页面 → 扩展检测 → 本地判定 → 同一个弹窗。

**规则只有一份**，在 Rust 里；扩展只负责看。

**没有做到的部分写在第 6 节**，没有藏。

---

## 附：关键文件位置

| 想了解 | 看这里 |
| --- | --- |
| 总进度 | `STATUS.md` |
| 各阶段细节 | `docs/development/phase-2a-progress.md` / `phase-2b-progress.md` / `phase-3a-progress.md` |
| 为什么这么设计 | `docs/decisions/` |
| 平台支持情况 | `docs/development/platform-matrix.md` |
| 推进流程 | `docs/development/` 各阶段进展文档 |
