# crates/storage — Phase 1 deliverable (not built in Phase 0)

## 为什么这个 crate 现在没有代码

Phase 0 曾实现过一个最小的 SQLite + SQLx 适配器（打开库、建表、写入、读回、
事务回滚、外键级联），但**在本机无法编译**：

```text
sqlx -> ... -> windows-sys -> dlltool -> "as.exe": No such file or directory
```

原因是本机使用 `x86_64-pc-windows-gnu` 工具链且缺少汇编器 `as.exe`，
而任何需要生成 Windows 导入库的 crate 都会因此失败。
证据与完整分析见 [ADR-0012](../../docs/decisions/0012-isolated-dev-toolchain.md)。

## 处理方式（诚实优先）

规范要求「不得把仅通过编译的功能标记为已通过实际运行测试」。
这里更进一步：**它连编译都没能通过**。因此：

- 该实现**不作为已完成代码保留**，避免给人「已经写好、只差跑一下」的错觉；
- `crates/storage` 在 `STATUS.md` 中保持 `Blocked`，并注明阻塞原因；
- crate 继续留在 workspace 里（保持目录结构与依赖边界），但不参与构建。

## 恢复条件

当满足以下任一条件时，Phase 1 重新实现并实测：

1. 本机具备 MSVC（VS Build Tools + Windows SDK）→ 改用 `x86_64-pc-windows-msvc`；
2. 或工具链补齐 `as.exe`（完整 MSYS2 / MinGW-w64 发行版）；
3. 或在 CI 的 Linux/Windows-MSVC runner 上执行。

## Phase 1 需要覆盖的内容（规范 §31.1）

- SQLx 迁移与事件持久化；
- 统计投影读写与读集加载（§4.3）；
- Outbox 表与状态机持久化（§16）；
- **事件、投影、Outbox 意图在同一事务内原子提交**（§4.3 / §16.1）；
- 崩溃恢复矩阵中「事务提交前 / 提交后消费前」两个时点（§32.5）。
