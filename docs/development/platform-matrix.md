# 平台兼容性矩阵

> 规范 §29.6。每项能力只能是：**原生支持 / 部分支持 / 需要桌面环境特定实现 / 仅能降级为系统通知 / 未验证**。
>
> **铁律**：没有在本机或 CI 真实测过的格子，一律填「未验证」。
>
> 最后更新：2026-11-16（Phase 0 结束）

## 图例

| 标记 | 含义 |
| --- | --- |
| ✅ 原生支持 | 已实测，本环境原生可用 |
| 🟡 部分支持 | 已实测，有条件可用 |
| 🔧 需特定实现 | 需按 DE / 合成器分别实现 |
| ⬇️ 仅能降级 | 已实测浮窗不可用 |
| ❓ 未验证 | **默认状态** |
| 🚫 不支持 | 已实测，结论否定 |
| n/a | 该能力对浏览器列不适用 |

## 矩阵

| 能力 | Windows 11 | macOS | Linux X11 | Wayland (GNOME) | Wayland (KDE) | Chrome | Edge | Brave |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 无边框透明浮窗 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 不抢焦点 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 点击穿透 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 置顶 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 全屏应用上方显示 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 系统托盘 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 系统通知 | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| 勿扰检测 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 多显示器 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| DPI 缩放 | ❓ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| 安装与卸载 | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| **Git Hook 行为** | ✅ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| **SQLite 持久化** | ✅ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| **WASM 编译与实例化** | ✅（Node 中） | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| **WASM 在扩展页内加载** | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| Native Messaging | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |

### 为什么这几格是 ✅

- **Git Hook 行为**：15 个场景在 Git 2.54.0 / Windows 11 上真实执行，原始 trace 在 `scripts/git-experiments/.out/`。
- **SQLite 持久化**：5 个真实数据库测试通过（往返、幂等、事务回滚、级联、重开保留）。
- **WASM 编译与实例化**：27,569 bytes 产物在 Node 中零导入实例化，且与 native 输出逐字节一致。

### 为什么 WASM 的「扩展页内加载」仍是 ❓

沙箱禁止 Chromium 创建 mojo 命名管道，浏览器**根本无法启动**。
manifest 里 `wasm-unsafe-eval` 已确认存在、wasm 已确认打包，但「在真实页面里能不能实例化」**没有实测**。
契约要求实测才能标 ✅，所以保持 ❓。见 [ADR-0001](../decisions/0001-wasm-strategy.md)。

## 本机可测性

| 平台 | 可测 | 说明 |
| --- | --- | --- |
| Windows 11 (build 26300) | ✅ | 本机；Rust / SQLite / Hook 全部实测 |
| macOS | ❌ | 无设备 |
| Linux X11 / Wayland | ❌ | 无设备 |
| Chrome 157 | ❌ **受限** | 已安装，但无法启动（mojo 通道被拒） |
| Edge 157 | ❌ **受限** | 同上 |
| Brave | ❌ | 未安装 |

## 更新规则

1. 每次实测后在「证据」追加：日期、环境、方法、结果。
2. 只有证据里有记录的格子才允许从 ❓ 变更。
3. 每个 Phase 结束时更新本文件并在 `STATUS.md` 中引用。

## 证据

| 日期 | 环境 | 能力 | 方法 | 结果 |
| --- | --- | --- | --- | --- |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Hook 触发（post-commit / post-rewrite / post-merge） | 15 场景真实仓库实验 | ✅ |
| 2026-11-16 | Windows 11 / Git 2.54.0 | `core.hooksPath` 自定义目录 | 指向共享目录后提交 | ✅ 钩子仍触发 |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Worktree 钩子 | `git worktree add` 后提交 | ✅ |
| 2026-11-16 | Windows 11 / Git 2.54.0 | 已有 shell 钩子的链式执行 | 放置 `#!/bin/sh` 钩子后提交 | ❌ **无法验证**：沙箱禁止 `sh.exe` |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Submodule | `git submodule add` | ❌ **无法验证**：内部依赖 `sh` |
| 2026-11-16 | Node 24 / wasm32-unknown-unknown | Core 编译为 WASM | `cargo build --target wasm32` | ✅ 27,569 bytes |
| 2026-11-16 | Node 24 | WASM 实例化 + 与 native 一致性 | `scripts/wasm-spike/run-wasm-spike.mjs` | ✅ 零导入、逐字节相同 |
| 2026-11-16 | Windows 11 / SQLite (bundled) | 持久化往返 | `cargo test -p devchievements-storage` | ✅ 5 tests |
| 2026-11-16 | Chrome 157 | MV3 加载扩展 | 4 种启动参数组合 | ❌ **无法验证**：mojo 通道创建被拒（0x5） |
| 2026-11-16 | Windows 11 / Git 2.54.0 | **原生 .exe hook 被执行** | 真实 `git commit`，hook 为 `post-commit.exe` | ✅ Git 直接执行，helper 收到仓库事实并写入 Spool |
| 2026-11-16 | Windows 11 / Git 2.54.0 | **POSIX shell hook** | 真实 `git commit`，hook 为 `#!/bin/sh` | ❌ **无法验证**：`sh.exe` 启动即崩（signal pipe 被拒，0xC0000142） |
| 2026-11-16 | Windows 11 | **shell 钩子链式执行** | 需要 sh 才能跑链式脚本 | ❌ **未验证**：依赖上一条 |
