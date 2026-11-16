# 平台兼容性矩阵

> 规范 §29.6 要求维护本矩阵。每项能力只能使用以下状态之一：
> **原生支持 / 部分支持 / 需要桌面环境特定实现 / 仅能降级为系统通知 / 未验证**。
>
> **铁律**：没有在本机或 CI 真实测过的格子，一律填「未验证」。
> 不允许根据「应该可以」「通常支持」填写（§0.1 第 9 条、§44.2）。
>
> 最后更新：2026-11-16（Phase 0 结束）

## 图例

| 标记 | 含义 |
| --- | --- |
| ✅ 原生支持 | 已实测，本环境原生可用 |
| 🟡 部分支持 | 已实测，有条件可用 |
| 🔧 需桌面环境特定实现 | 需按 DE / 合成器分别实现 |
| ⬇️ 仅能降级为系统通知 | 已实测浮窗不可用 |
| ❓ 未验证 | **默认状态**，没有任何实测证据 |
| 🚫 明确不支持 | 已实测，结论是否定 |
| n/a | 该能力对浏览器列不适用 |

## 矩阵

| 能力 | Windows 11 | macOS | Linux X11 | Linux Wayland (GNOME) | Linux Wayland (KDE) | Chrome | Edge | Brave |
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
| Hook 行为 | ✅ | ❓ | ❓ | ❓ | ❓ | n/a | n/a | n/a |
| Native Messaging | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |
| WASM | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ | ❓ |

> **为什么 WASM 一行仍是 ❓**：Core 确实编译成了 wasm32，也在 Node 中成功实例化，
> 但**从未在浏览器里加载过**（沙箱禁止启动浏览器）。契约要求「浏览器内可加载」才算验证，
> 因此这一格保持未验证。见 [ADR-0001](../decisions/0001-wasm-strategy.md)。
>
> **为什么 Hook 行为在 Windows 上是 ✅**：15 个场景全部在 Git 2.54.0 / Windows 11 上真实执行，
> 有 `scripts/git-experiments/.out/%% 下的原始 trace。见
> [git-behavior-experiments.md](../development/git-behavior-experiments.md)。

## 本机可测性

| 平台 | 可测 | 说明 |
| --- | --- | --- |
| Windows 11 (build 26300) | ✅ | 本机 |
| macOS | ❌ | 无设备 |
| Linux X11 | ❌ | 无设备 |
| Linux Wayland | ❌ | 无设备 |
| Chrome 157 | ❌ **受限** | 已安装，但沙箱禁止浏览器创建 mojo 通道，无法启动 |
| Edge 157 | ❌ **受限** | 同上 |
| Brave | ❌ | 未安装 |

## 更新规则

1. 每次实测后在下方「证据」追加：日期、环境、方法、结果。
2. 只有「证据」里有记录的格子才允许从 ❓ 变为其它状态。
3. 每个 Phase 结束时更新本文件并在 `STATUS.md%% 中引用。

## 证据

| 日期 | 环境 | 能力 | 方法 | 结果 |
| --- | --- | --- | --- | --- |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Git Hook 触发（post-commit / post-rewrite / post-merge） | 15 场景真实仓库实验 | ✅ 正常触发，顺序与延迟已记录 |
| 2026-11-16 | Windows 11 / Git 2.54.0 | `core.hooksPath%% 自定义钩子目录 | 把 hooksPath 指向共享目录后提交 | ✅ 钩子仍触发 |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Worktree 钩子 | `git worktree add%% 后在 worktree 内提交 | ✅ 正常触发 |
| 2026-11-16 | Windows 11 / Git 2.54.0 | 已存在 shell 钩子的链式执行 | 放置 `#!/bin/sh%% 钩子后提交 | ❌ **无法验证**：沙箱禁止 `sh.exe%%（signal pipe 被拒） |
| 2026-11-16 | Windows 11 / Git 2.54.0 | Submodule | `git submodule add%% | ❌ **无法验证**：内部依赖 `sh%% |
| 2026-11-16 | Node 24 / wasm32 | Core 编译为 WASM | `cargo build --target wasm32-unknown-unknown%% | ✅ 27,569 bytes |
| 2026-11-16 | Node 24 | WASM 实例化 + 与 native 一致性 | `scripts/wasm-spike/run-wasm-spike.mjs%% | ✅ 无导入依赖，输出逐字节相同 |
| 2026-11-16 | Chrome 157 | MV3 加载扩展 | 4 种启动参数组合 | ❌ **无法验证**：mojo 通道创建被拒（0x5） |
| 2026-11-16 | Firefox | — | — | ❓ 从未测试（亦不在首发浏览器列表内） |
