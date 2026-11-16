# CI 第一次真实运行（2026-11-17）

> 这份文件记录 CI 首次执行的结果与修复，因为工作流本身在提交时是**从未运行过**的配置。

## 结果

失败，两个独立原因。

## 1. Node job：pnpm 版本冲突

```text
Error: Multiple versions of pnpm specified
```

`pnpm/action-setup` 里写了 `version: 10`，而 `package.json` 里有 `packageManager`，
两者同时存在时 action 直接拒绝。

修复：删掉 action 里的 `version`，只保留 `packageManager` 一个事实来源。

## 2. Rust job：commit 解析只认松散对象

三个平台都在同一个测试上失败：

```text
reading_a_commit_survives_git_gc: Malformed("missing object header")
```

根因：

| 路径 | 输入形状 |
| --- | --- |
| 松散对象 | `commit <size>\0<payload>` |
| pack 对象 | `<payload>`（没有头） |

`parse_commit_object` 要求必须有 `\0` 分隔，于是**所有从 pack 读的提交都失败**。

本地一直是绿的，因为本地 git 2.54 的 `gc --aggressive` 没有让这个对象走 pack 路径；
CI 上老版本 git 先打包了它。

修复：两种形状都接受，并用 `tree ` 前缀作为「这确实是个提交」的判据。

新增 `crates/git-hooks/tests/commit_object_shapes.rs`，3 个测试直接锁住这两个形状。

## 教训

「本地全绿」在跨平台 CI 上不是证据。这次的 bug 一直是存在的，
只是本地环境的执行顺序恰好绕开了它。

## 仍然未验证

- `git-version-matrix` job 并没有真的安装指定版本的 Git，只是打印版本号。
  它是**诚实的占位**，不是通过的测试，STATUS 里也这么记。
- 桌面构建（Tauri/MSVC）不在 CI 里：它依赖本机的 MSVC 环境，尚未移植到 runner。
