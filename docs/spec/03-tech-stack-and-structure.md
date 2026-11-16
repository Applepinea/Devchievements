# 第 3 章　技术栈与项目结构

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 3. 技术栈与项目结构

## 3.1 技术栈

| 模块 | 技术 |
| --- | --- |
| Desktop | Tauri 2、Rust、React、TypeScript、CSS、Framer Motion |
| Browser | WXT、Manifest V3、TypeScript、React |
| Storage | SQLite + SQLx（Desktop）；IndexedDB（Browser Only） |
| Communication | Native Messaging（扩展与 Companion）；本地 IPC（Hook Helper 与 Companion） |
| Workspace | pnpm Workspace + Cargo Workspace |
| Core | Rust Core Engine，尽可能通过 WASM 共享到浏览器 |
| Testing | Rust Tests、Vitest、Playwright、Golden Tests |
| CI/CD | GitHub Actions |

- 禁止在同一项目中混用 SQLx 和 rusqlite，除非有 ADR 证明必要。
- 具体依赖版本在项目创建时选择稳定版本并记录。

## 3.2 Monorepo 结构

```text
devchievements/
├── apps/
│   ├── desktop/
│   ├── extension/
│   └── native-host/
├── crates/
│   ├── core/  events/  achievements/  daily-rank/
│   ├── notifications/  sound/  assets/  git-hooks/
│   └── storage/  filters/  themes/  platform/
├── packages/
│   ├── shared-types/  ui/  config/
│   └── golden-tests/        # 含 fixtures/git-traces/
├── docs/
│   ├── spec/  decisions/  development/
├── tests/  scripts/
├── AGENTS.md  STATUS.md  README.md
└── Cargo.toml  pnpm-workspace.yaml
```

Agent 可以根据依赖关系调整目录，但必须保留清晰的模块边界。

## 3.3 WASM 技术验证

Phase 0 必须进行独立 WASM spike，验证：Rust Core 能否编译为 WASM；目标浏览器的 Manifest V3 环境能否加载（包括 CSP 中的 `wasm-unsafe-eval` 配置，必须在目标浏览器实际验证）；序列化行为是否一致；时间、时区和日期逻辑是否一致；Golden Tests 能否在 Native 与 WASM 下得到相同结果；性能是否满足需求。

决策：

- Spike 成功：Native 与 Browser 共用同一 Rust Core Engine。
- Spike 失败：Rust Native Engine + TypeScript Browser Engine，两者必须通过同一套 Golden Test Suite。

不得为了坚持 WASM 而牺牲浏览器兼容性、稳定性或开发质量；决策结果与测试证据写入 ADR。

---
