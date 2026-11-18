# 首屏性能：基线、改动与实测结果

> 依据：对外部审查意见的逐条落实。**先测量再优化**——审查本身也要求先建基线（P0）。
> 全部数字来自 `scripts/perf/first-screen.mjs`，原始样本在 `scripts/perf/.out/first-screen.json`。

## 1. 测量方法

GUI 子系统的进程没有控制台，会把输出重定向到 `companion.log`。**读启动器的 stdout 只能拿到 0 个样本**——
这是第一版测量脚本的错误，也是「measure first」的直接证据：不测量就不会发现连测量本身都是错的。

两个模式，因为它们本来就是两个问题：

| 模式 | 含义 |
| --- | --- |
| **cold** | 进程启动 → 窗口打开。包含 WebView2 与进程启动 |
| **warm** | 应用已在托盘，第二次启动只是请求显示窗口 |

阶段（页面侧 `performance.now()`，进程侧 `Instant`）：

```text
script_start    页面 JS 开始执行
boot_resolved   语言与 onboarding 状态到达（一次小 IPC）
shell_ready     外壳报告就绪
dashboard_data  卡片拿到数字
```

## 2. 实测结果（release 构建，同一台机器，各 12 次）

| 阶段 | cold p50 前 → 后 | cold p95 前 → 后 | warm p50 前 → 后 | warm p95 前 → 后 |
| --- | --- | --- | --- | --- |
| shell_ready | **848 → 371 ms** | 1197 → **463 ms** | **493 → 48 ms** | 836 → **96 ms** |
| dashboard_data | **855 → 376 ms** | 1204 → **501 ms** | **504 → 49 ms** | 848 → **116 ms** |
| 请求→数据（warm） | — | — | **879 → 350 ms** | 1259 → **444 ms** |

**分布不重叠**：改动后最慢的 cold 样本（`max` 463 ms `shell_ready`）仍快于改动前最快的样本（`min` 778 ms）。
这不是噪声范围内的差别。

## 3. 改了什么

| 优先级 | 改动 | 依据 |
| --- | --- | --- |
| P1 | `boot_status`：只返回 `language` + `onboardingCompleted` | `get_settings` 解析整份设置，`onboarding_status` 跑多次 Git 查询 + helper 检查——已完成 onboarding 的用户每次开窗都在为向导付钱 |
| P1 | 外壳立刻渲染骨架，不再等两个 IPC | 「等两项都返回才离开 Starting…」是可避免的最大一块 |
| P1 | 页面级 `React.lazy` + `Suspense` | 打开仪表盘不再解析另外四个页面的 JS。Dashboard 保持同步加载：给它分包反而给首屏加一次往返 |
| P2 | `get_dashboard` 拆出 `get_storage_health` | 路径/大小/schema 是诊断信息，不该压着数字不显示 |
| P2 | SQLite pool 缓存（`OnceLock`） | 原来**每个页面请求**都新开一个 pool 并跑一次迁移检查 |
| P3 | setup 阶段耗时测量 | 实测 autostart **0.07 ms**、helper **7.26 ms** → 不值得移出启动路径，**未改动** |

## 4. 分块结果

```text
index-*.js        245.59 kB │ gzip 80.15 kB     ← 入口，含 Dashboard
Welcome-*.js        7.77 kB │ gzip  2.50 kB
Settings-*.js      17.76 kB │ gzip  4.62 kB
About-*.js          3.99 kB │ gzip  1.15 kB
Repositories-*.js   1.98 kB │ gzip  0.89 kB
Gallery-*.js        1.34 kB │ gzip  0.62 kB
Activity-*.js       1.09 kB │ gzip  0.54 kB
```

入口从「包含全部页面」变成了只含 Dashboard；其余按需加载。

## 5. 懒加载的一处真实代价

`preview-button` e2e 在改动后**失败**了：它等的是「页面上出现任意按钮」，而导航现在先渲染、
设置页的内容随后才到——于是测试在 chunk 边界上抢跑了。

这不是测试脆弱，是**懒加载改变了可观察的时序**。修法是把等待条件改成「等到这个页面自己的控件出现」，
而不是放宽超时。这条值得记下来：分包会把「页面可用」拆成两个时刻。

## 6. 未做与建议

| 项 | 状态 |
| --- | --- |
| `daily_projection` 的日期索引 | **未做**。审查建议先 `EXPLAIN QUERY PLAN` 再决定；当前数据量（数十行）下无法测出收益，加了反而增加写入成本 |
| `events` 计数移出首屏响应 | **未做**。它在同一条 SQL 里，去掉不减少查询次数；真正该做的是合并统计 SQL，需要单独测量 |
| 并发读取多个独立查询 | **未做**。同上，先有查询级测量再动 |
| 首次 onboarding 路径 | **未测**。本次只覆盖已完成 onboarding 的用户；向导路径包含 Git 探测，应单独测 |
