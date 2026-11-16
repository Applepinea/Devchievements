# 第 10 章　本地传输：IPC 优先，Spool 回退

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 10. 本地传输：IPC 优先，Spool 回退

## 10.1 传输路径

```text
Hook → Rust Helper → 本地 IPC（短超时）→ Companion
                        │ 失败或超时
                        ▼
                     Spool（追加式）→ Companion 启动或恢复后消费
```

- Helper 先尝试 IPC：连接超时默认 30 ms，等待 Companion 确认默认 50 ms；任一步失败即写入 Spool。
- 投递语义为至少一次加幂等：IPC 已到达但确认丢失时，Spool 中的同一幂等 ID 会被 Companion 去重。
- 事件携带：幂等 ID、仓库身份、Commit SHA、Hook 类型、Hook 触发时间、必要事件数据、Schema 版本。
- Companion 启动时消费遗留 Spool，运行期间监视 Spool 目录以处理回退写入。

## 10.2 IPC 安全

- Windows：Named Pipe，ACL 限制为当前用户。
- macOS / Linux：Unix Domain Socket 放在当前用户私有目录（目录 0700，Socket 0600）；Linux 优先使用 `XDG_RUNTIME_DIR`。
- 平台支持时校验对端凭据（如 SO\_PEERCRED、LOCAL\_PEERCRED、命名管道客户端进程信息）。
- 校验消息大小、结构、类型、版本；设置读写超时；拒绝其他用户来源。
- IPC 不执行任何命令，不携带 Token 或密钥；不得使用无保护的 localhost HTTP。

## 10.3 Spool 基本能力

追加式原子写入；文件锁；多进程并发安全；每条记录带校验信息；损坏检测与恢复；大小上限与清理；崩溃恢复；已处理记录清理。

## 10.4 损坏处理

发现损坏记录时：隔离损坏数据；记录诊断信息；尽可能恢复后续有效记录；不得因单条损坏导致整个 Spool 无法读取。

## 10.5 大小限制与清理

必须设置合理的上限。达到上限时：明确记录错误；给出用户可见的状态反馈；不得静默丢弃事件；不得清理尚未确认持久化的事件。只有确认完成处理的记录才允许清理。

## 10.6 并发与原子性

多个 Hook 同时写入不得造成记录交叉、覆盖或损坏。文件锁和写入协议必须经过并发测试。

---
