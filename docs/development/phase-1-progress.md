# Phase 1 进展（Core Engine 与数据层）

- **日期**：2026-11-16
- **状态**：**In Progress** —— 核心逻辑层已完成并通过测试；剩余的是存储投影与历史导入管线
- **验证**：`node scripts/verify.mjs` → **147 tests passed, 0 failed**

---

## 1. 本阶段完成了什么

| 模块 | 交付 | 测试数 |
| --- | --- | ---: |
| `crates/events` | 标准事件模型、来源可信度、仓库身份、验证规则 | 31 |
| `crates/core`::timezone | IANA 时区 + 日界线 + 内置 TZDB | 12 |
| `crates/core`::engine | Engine（含读集契约） | 7 |
| `crates/daily-rank` | 每日等级、进度、升级判定 | 14 |
| `crates/achievements` | 永久成就、版本化规则、回填策略、能力声明 | 13 |
| `crates/notifications` | Outbox 状态机、通知合并、新鲜度 | 18 |
| `crates/golden-tests` | 回放 Phase 0 的 15 份 Git trace | 16 |
| `crates/storage` | SQLite + SQLx：schema、事务化摄取、投影、历史导入、IndexedDB 接口 | 14 |
| `crates/git-hooks` | 直读 .git 的 Helper | 22 |
| 其他 crate | 版本常量占位 | 9 |

---

## 2. 几个值得单独说明的设计决定

### 2.1 时区：内置 TZDB，而不是读系统库

读操作系统时区库意味着 Windows 更新可能改变一个历史事件映射到哪一天，
从而**静默改写统计**。内置 `jiff-tzdb` 让「重放稳定」成为构建的属性，而不是指望。

实测：ADR-0005 曾担心 `jiff` 在本机 GNU 目标下无法构建。**已实测可以**，
并验证了 `Asia/Taipei` 与 `America/New_York` 的 DST 边界。

### 2.2 成就口径与用户过滤器在类型上分离

`StatisticsProfile::Official` 与 `UserFiltered` 是两个不同的值，
内置成就**只能**是前者。所以「用户把 Legendary 阈值改成 15」这件事在类型层面
就不可能影响内置成就，而不是靠人记得别写错。

有一条测试专门盯着这个：`custom_thresholds_do_not_change_official_ones`。

### 2.3 能力声明由条件推导，而不是手工维护

`RuleCondition::required_capability()` 从条件本身推出需要 LocalGit 还是
BrowserGithub。这样 `required_capabilities` 不可能与规则实际需求脱节——
手工维护的清单迟早会漂移。

### 2.4 Engine 的读集是显式常量

`ReadSet` 列出一次评估允许接触哪些投影。有一条测试断言读集覆盖了 Engine
实际用到的每一个投影；一旦有人加了新依赖却忘了声明，测试就会失败。
这直接对应 §4.3「不得每次 Commit 都扫描所有历史」。

### 2.5 通知状态机：把「宁可少通知」编码进表

§17.5 / §17.9 的原则是**宁可少通知，不重复通知**。所以：

- `delivery_unknown` 是终态，**不会**回退到桌面；
- 桌面展示前必须先持久化 `display_attempted_at`，崩溃恢复时据此拒绝重播；
- 合法转换集中在一张表里，任何越界转换返回错误而不是静默接受。

---

## 3. 过程中修掉的真实缺陷（不是笔误，是逻辑问题）

| 问题 | 怎么发现的 | 处理 |
| --- | --- | --- |
| 时区测试用了 2025 年的 epoch 常量 | 断言失败，报 2025-11-16 | 用 `Date.UTC(2026,10,16)` 重算并注明推导方式 |
| DST 测试按「+24 小时」遍历 | 跨入第二天，断言全错 | 改为遍历**同日历日**的各小时（DST 日只有 23 小时） |
| `delivery_unknown` 允许从 `offered` 进入 | 测试失败；对照 §17.3 确认只允许从 `claimed` | 测试前提写错，并补一条测试锁住正确规则 |
| 每日等级测试断言「第二次提交不应再通知」 | 实际会升到 Uncommon（正确行为） | 断言写错；改为只断言 Common 不重复、Uncommon 必须通知 |
| 阈值校验的 `ZeroMinimum` 分支不可达 | 单元测试失败 | 承认它被「必须从 1 开始」先拦下，保留变体并说明 |
| `RuleCondition` 缺 serde derive | 编译失败 | 补 derive；这是「含在 `Achievement` 里」的必然结果 |
| `StateSnapshot` 含 Vec 却标了 Copy | 编译失败 | 去掉 Copy |
| 移除 `workspace.dependencies` 导致 native-host 无法加载 | 全量验证 5 项失败 | 恢复最小表（只列真正被 `workspace = true` 消费的项） |

---

## 4. 尚未完成（Phase 1 剩余）

| 项 | 为什么还没做 |
| --- | --- |
| 统计投影的持久化读写 | 需要把 Engine 输出接到 storage 的事务里 |
| 历史导入管线（分批、静默、幂等） | 依赖投影落地 |
| 用户过滤器 | 独立模块，尚未实现 |
| packfile 解析 | **风险最高**的一项（见 risk-analysis A3），建议优先 |
| 属性测试（时区、乱序 Hook） | 已有个例测试，缺系统性属性测试 |
| 用户过滤器 | 独立模块，未实现 |

---

## 5. 关于「147 个测试」的诚实说明

这些是**真实的单元与集成测试**，不是编译检查：

- 时区测试跑真实 TZDB 并断言具体的日期字符串；
- SQLite 测试跑真实数据库文件（往返、回滚、级联、重开）；
- Golden Tests 读的是 Phase 0 真机录制下来的 trace，不是编造的 fixture。

但仍然**没有**验证到的：

- 桌面端到端闭环（无 Tauri，无 MSVC）；
- 浏览器内 WASM（沙箱禁止启动浏览器）；
- 真实 Git 仓库上的长期运行（只有单次 commit 路径）。

---

## 6. 本轮补完的部分（投影持久化 → 历史导入）

### 6.1 一个事务里做完三件事

规范 §4.3 / §16.1 要求：原始事件、投影更新、Outbox 意图**同一事务提交**。
`ingest::ingest_commit` 把这三步包在一个 SQLite 事务里，任何一步失败整体回滚。

有一条测试盯着这个：`ingesting_a_commit_writes_event_projection_and_outbox_together`
断言事件行、投影行、成就解锁行、Outbox 行**同时**存在。

### 6.2 幂等是数据库主键保证的，不是靠调用方小心

`event_id` 是主键。重复投递返回 `IngestOutcome::Duplicate` 且**什么都不改**。
测试 `re_ingesting_the_same_event_is_a_no_op` 同时断言「不新增事件」和「投影不重复计数」。

### 6.3 投影是可重建的

`rebuild_from_events` 从事件日志重算所有投影。有测试断言重建后数字与实时累积**完全一致**。
这条性质是「将来改口径」的前提——否则历史统计一旦算错就永远错了。

### 6.4 历史导入用提交时间，不用导入时间

§11.2 的核心要求。测试里特意选了一个跨日时刻：
`1700000000000` = `2023-11-14T22:13:20Z`，在 `Asia/Taipei` 是 **2023-11-15**。

**写这个测试时我自己先写错了**（断言成 11-14），跑失败才纠正。
这个错误恰好演示了 §12 存在的理由：用 UTC 日期归档会错一天。

### 6.5 初步性能基准（§31.3 要求）

`cargo test -p devchievements-storage --release --test perf -- --ignored --nocapture`，
n = 2000 次真实摄取（含校验、去重、写事件、更新投影、跑 Engine、写 Outbox、提交）：

```text
min   1.376 ms
p50   3.045 ms
p95   3.117 ms
max  13.982 ms
throughput 323 events/sec
```

**这不能用来声称「Commit → 通知 p95 < 500 ms 达标」**：它只覆盖存储 + Engine 这一段，
没有 Companion、没有 IPC、没有弹窗渲染。基准里也写明了这一点。

### 6.6 IndexedDB 只做接口，不做实现

§31.1 明确「接口设计（不展开浏览器协作）」。所以 `indexeddb.rs` 只有 trait 和
对象存储名常量——**刻意把对象存储名固定下来**，让桌面 schema 与浏览器 schema 不能悄悄分裂。


---

## 7. 本轮（按顺序）完成的最后三项

### 7.1 用户过滤器（§15）

`crates/filters`，18 个测试。核心是把两种口径**在类型上分开**：

`StatisticsScope::Official` 完全忽略过滤器，`StatisticsScope::UserFiltered` 才应用。
有一条测试专门断言：「排除了全部仓库之后，用户统计为 0，但官方统计仍是 1」。

这条保证如果靠注释维持，迟早会被人改坏。

### 7.2 属性测试（§12、§9.6 第 7 条）

`crates/property-tests`，11 个属性，用 `proptest`。

- 时区 5 条：确定性、边界只向前移且不超过一天、午夜边界稳定、时间单调、未知时区不猜。
- Hook 6 条：任意到达顺序收敛、重写不重复计数、整段重复投递幂等、新鲜度只依赖事件年龄。
- 通知 1 条：任意消息顺序下**最多一个终态**。

**属性测试第一版是被自己写错的模型坑了**：我原来的 `apply_order` 在
「重写先到」分支里把新旧 sha 都塞进计数集合。测试失败后重写模型才对。
这说明属性测试确实在测东西 —— 它测出了一个我自己没想到会错的模型。

### 7.3 Packfile（§6.2，风险 A3）

先复现故障再修：`git gc` 之后 loose 目录归零，原实现直接报「找不到路径」。

实现后，在**真实 gc 过的仓库**上验证：10 个历史提交逐个可读；
截断 pack 后 loose 的 HEAD 仍可读。见 [ADR-0013](../decisions/0013-packfile-support.md)。

**没有使用手工构造的 pack 自证** —— 那只能证明我对格式的理解自洽。

---

## 8. Phase 1 完成度

| §31.1 要求 | 状态 |
| --- | --- |
| 标准事件模型与验证 | ✅ |
| 来源可信度 | ✅ |
| 仓库身份识别 | ✅ |
| 用户身份识别（多邮箱） | ⚠️ 仅有邮箱匹配函数，未接 Git 配置读取 |
| Commit 去重与统计规则 | ✅ |
| 时区系统 | ✅ |
| 每日等级与里程碑 | ✅ |
| 永久成就（含 required_capabilities） | ✅ |
| 用户过滤器 | ✅ |
| 统计投影 | ✅ |
| Engine（含读集） | ✅ |
| SQLite Storage Adapter 与迁移 | ✅ |
| 事件持久化 | ✅ |
| Outbox 与通知所有权状态机 | ✅ |
| 历史导入管线 | ✅ |
| Golden Tests | ✅ |
| IndexedDB Adapter 接口设计 | ✅ |
| Achievement Builder 复杂度限制 | ⚠️ 未做（属 §22，Phase 4a 主体） |
| 时区属性测试 | ✅ |
| 初步性能基准 | ✅ |

**仍未做的两项**：用户身份读取 Git 配置、Achievement Builder 的复杂度限制。
两者都不阻塞 Phase 2a。


---

## 9. 遗留问题已处理

### 9.1 用户身份识别（§7）—— 补完

上一版只有 `author_matches` 一个函数。现在补齐为完整模型：

- `Identity`：多邮箱集合 + 每个邮箱的可信来源（UserConfigured / RepositoryConfig / GlobalConfig / GitHubNoreply）
- **优先级可排序**，且同优先级时保持插入顺序（否则诊断不可复现）
- GitHub login → 邮箱映射**必须显式**，有测试断言「不会把 login 当成邮箱前缀来猜」
- 读取 Git 本地/全局配置作为**建议**，带来源标记，不直接当成身份

**最关键的一条**：`is_configured` 接进了 `ingest_commit`，是**第一个检查**，
在写库之前。有 3 个测试盯着：未配置拒绝、别人的提交拒绝、无作者的 GitHub 事件放行。

这条规则以前只写在文档里，现在是代码。

### 9.2 Achievement Builder 复杂度限制（§22）—— 补完

**我上一版把它误判成 Phase 4a 了。** §22 明确写着「Phase 1 实现这些限制并附测试」。

实现：

| 限制 | 值 | 是否可配置 |
| --- | --- | --- |
| 最大嵌套深度 | 5 | 可配置，但 `clamped()` 强制硬上限 |
| 最大条件数 | 50 | 同上 |
| 单次评估步数 | 1000 | 同上 |

**用步数预算而不是墙钟超时**。§22 明确禁止墙钟，因为它会破坏确定性。
有测试跑 10 次断言结果完全相同。

超出预算时返回 `exhausted = true`，**不是**「未满足」——后者会让配置错误看起来像规则不适用。

规则之间不允许相互引用，所以循环引用在结构上不可能存在（没有引用机制）。
