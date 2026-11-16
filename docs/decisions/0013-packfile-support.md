# ADR-0013：Packfile 与 Delta 解析

- **状态**：**Accepted**
- **日期**：2026-11-16
- **相关规范章节**：§6.2、§9.5、§31.1
- **前置**：[ADR-0002](0002-hook-helper-implementation.md)（为什么直读 `.git`）
- **风险登记**：[risk-analysis.md](../development/risk-analysis.md) A3

---

## 背景

ADR-0002 选择直读 `.git`，Phase 0 的实现只解析 **loose object**。

风险分析 A3 当时就写了：真实仓库迟早会被 `git gc`，届时对象全部进入 packfile，
当前实现会失败。**这不是理论风险，已实测复现**：

```text
gc 前 loose 目录数：15
git gc --aggressive --prune=now
gc 后 loose 目录数：0
pack 文件：pack-45b4e00c....pack

读 HEAD：
{"ok":false,"error":"commit: loose object (.../objects/7d/b0bb...):
 系统找不到指定的路径。 -- packed objects are not handled by this spike"}
```

即：**功能会在用户正常使用一周后静默失效**。

---

## 决策

**实现 packfile 与 delta 解析，而不是回退到调用 `git`。**

理由：

- ADR-0002 的实测结论是直读比外部调用快 4 倍，且不引入子进程；
- 回退到 `git` 会让「对象在 pack 里」成为一条高频路径，而不是罕见兜底；
- pack 格式是稳定且公开的，实现成本可控。

---

## 实现范围

| 能力 | 状态 |
| --- | --- |
| pack index **v2** 解析（magic、fanout、oid 表、大偏移表） | ✅ |
| pack 头与对象头解析（type 3 位 + 变长 size） | ✅ |
| 对象体 zlib inflate | ✅ |
| **OFS_DELTA**（包内负偏移） | ✅ |
| **REF_DELTA**（按 oid 引用，含跨对象链） | ✅ |
| delta 链递归（深度上限 128） | ✅ |
| 多 packfile（逐个搜索） | ✅ |

**明确不支持，且必须报错而不是猜测**：

| 不支持 | 原因 |
| --- | --- |
| pack index **v1** | 2007 年前格式；本机无法构造样本验证，宁可报错 |
| **thin pack** | 引用包外对象；目前视为错误并给出明确信息 |
| SHA-256 仓库 | 与 loose 路径的限制一致 |

---

## 容错设计

两条原则：

1. **单个坏 pack 不得让整个仓库不可读。** `read_from_packs` 对每个 pack 独立
   `open`，失败就跳过继续找。有测试专门截断 pack 后验证 loose 的 HEAD 仍可读。
2. **「对象不存在」与「对象读不出来」必须区分。** 前者是 `PackedObjectNotSupported`，
   后者是 `Pack(...)` 并带原因。把它们混成一个 "no commit" 就是静默丢事件，规范禁止。

---

## 验证

全部在**真实执行过 `git gc` 的仓库**上测试（`crates/git-hooks/tests/packfile.rs`）：

| 测试 | 断言 |
| --- | --- |
| `reading_a_commit_survives_git_gc` | gc 后 loose 目录数为 0，HEAD 仍可读且字段正确 |
| `reading_an_older_commit_from_history_survives_gc` | 10 个历史提交**逐个**可读（这些最可能被 delta 压缩） |
| `a_pack_exposes_its_objects_by_id` | 索引列出 HEAD，解码出的对象以 `tree ` 开头 |
| `a_missing_object_still_reports_a_precise_error_after_gc` | 不存在的对象报精确错误，不假装「没有提交」 |
| `the_repository_still_works_when_one_pack_is_corrupt` | 截断 pack 后，loose 的 HEAD 仍可读 |

另有 10 个 delta 单元测试覆盖：copy/insert 组合、基长不匹配、copy 越界、
opcode 0、多字节 varint、截断 varint、oid 解析长度。

---

## 代价与新风险

- **代码量**：约 700 行，含完整 delta 应用与索引解析。
- **正确性风险**高于调用 `git`，缓解方式是**只在真实 gc 仓库上测**，
  而不是用手工构造的 pack 自证。
- **性能**：每次 `read_at` 都重新打开文件。对一个 Hook 来说只有几次调用，
  实测仍在预算内；如果要读大量对象（历史导入），应改为复用一个 file handle。
  **这一点尚未优化，也尚未测量**，属于已知待办。

---

## 复查计划

Phase 2a 的历史导入会大量读对象，届时测量 pack 读取吞吐；
若不满足，再引入 handle 复用或浅层 pack 缓存。
