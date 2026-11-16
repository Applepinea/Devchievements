# ADR-0007：IPC 与 Spool 传输设计

- **状态**：**Accepted**
- **日期**：2026-11-16
- **相关规范章节**：§10、§9.2、§32.2、§32.5
- **前置**：[ADR-0002](0002-hook-helper-implementation.md)（Helper 为什么直读 .git）

---

## 背景

Git Hook 里能做的事情有严格预算：§44.1 要求 p95 < 50 ms，§9.2 要求硬超时 200 ms，
且**任何失败都不得影响用户的提交**。

同时 Companion 可能没在运行。事件不能丢。

---

## 决策

### 1. 传输：IPC 优先，Spool 兜底（§10.1）

```text
Hook → Helper → IPC（连接 30 ms / 确认 50 ms）
                 │ 失败或超时
                 ▼
              Spool（追加式）→ Companion 启动或恢复后消费
```

**关键取舍：超时极短。** 30 + 50 = 80 ms 已经是 50 ms p95 预算的 1.6 倍，
所以 IPC 实际上是「本地快速通道」，任何不确定就立刻放弃并写 Spool。
Spool 写入是本地的、可控的、可测量的。

### 2. 平台原语

| 平台 | IPC | 权限 |
| --- | --- | --- |
| Windows | Named Pipe `\\.\pipe\devchievements-<user-sid-hash>%% | 默认 owner-only ACL；显式拒绝远端 |
| Linux | Unix domain socket in `$XDG_RUNTIME_DIR%% | 目录 0700，socket 0600 |
| macOS | Unix domain socket in user-private dir | 同上 |

**绝不使用 localhost TCP。** §10.2 明确禁止「无保护的 localhost HTTP」，
而裸 TCP 与它的风险等价：任何本地进程都能连接。

### 3. 身份校验

- Unix：`SO_PEERCRED%% / `LOCAL_PEERCRED%% 校验对端 UID 等于当前 UID。
- Windows：Named Pipe 的默认安全描述符已限制为创建者；额外校验客户端进程的
  session id 相同（同一登录会话）。
- 消息带长度上限（64 KiB）与 schema 版本，超限直接断开而不是读取。

### 4. Spool 格式：一行一条，带校验

```text
<32-hex record checksum> <json payload>\n
```

- **追加写**（`O_APPEND%%），单次 `write%% 一条记录，避免多进程交叉。
- 每行自带校验：损坏的行被**跳过并隔离**，不影响后续记录（§10.4）。
- 已消费的记录**不是删除行**，而是把整个文件重命名进 `.consumed/%%，
  再由 Companion 按需清理。这样崩溃不会造成「删了一半」。

### 5. 幂等（§10.1 的「至少一次 + 幂等」）

- 每条记录带 `event_id%%。
- Companion 侧 `events.event_id%% 是主键，重复插入返回 Duplicate 且不改任何投影。
- 因此「IPC 成功但确认丢失 → Spool 又写了一遍」是安全的。

### 6. 大小上限与清理（§10.5）

- 默认上限 16 MiB 或 20000 条，先到者为准。
- 达到上限：**记录错误 + 状态反馈**，不静默丢弃（§10.5 明确要求）。
- 超过 7 天的已消费文件被清理。

---

## 为什么不把「通知」也走 Spool

Spool 的职责是**可靠地把事件送达 Companion**，不是通用消息队列。
通知是 Companion 内部的 Outbox 事务产物（§16.1），走另一条路。

---

## 后果

### 正面

- Hook 最坏路径是「一次本地文件写入」，可测量、可预测。
- Companion 离线时事件不丢，重启后自动补上，且新鲜度策略照常生效
  （两天前的提交不会弹庆祝）。

### 负面 / 代价

- **至少一次语义**意味着必须靠 `event_id%% 幂等来兜底；任何绕过那个主键的写入路径
  都会引入重复计数。这一点在 storage 层已经用主键 + 测试锁死。
- Spool 需要一个「已消费」目录，带来一点磁盘管理复杂度。
- Windows Named Pipe 的安全性依赖默认 ACL；**未能实测跨用户攻击**（见下）。

### 未验证（诚实记录）

- **跨用户连接尝试未被实际攻击测试。** 本机只有一个用户会话，无法真实模拟
  另一用户的连接。ADR 与 STATUS 都标记为未验证。
- macOS 的 `LOCAL_PEERCRED%% 路径**从未在 macOS 上编译或运行**。
