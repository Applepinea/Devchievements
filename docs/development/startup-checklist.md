# Agent 启动检查清单（Startup Checklist）

> 每次开始一个新会话 / 新 Phase / 一段较长的中断之后，**按顺序执行本清单**。
> 目的：避免读到过期状态、避免重复劳动、避免在错误的环境假设上写代码。

---

## 0. 先读什么（强制顺序）

1. **本仓库根 [AGENTS.md](../AGENTS.md)** —— 强制约束，读完再动手。
2. **[STATUS.md](../STATUS.md)** —— 当前唯一权威进度表。**不要**相信自己的记忆，只相信它。
3. **[docs/spec/README.md](../spec/README.md)** —— 规范章节索引，按需跳转。
4. 当前 Phase 对应的规范章节（例如 Phase 0 → `30-phase-0.md`）。
5. **[docs/decisions/](../decisions/)** 中与本任务相关的 ADR（尤其是 Status = Accepted 的）。
6. **[docs/development/environment-baseline.md](environment-baseline.md)** —— 本机环境事实（**不是通用假设**）。

## 1. 环境自检（每次新 Shell 必做）

```powershell
Set-Location <repo root>
. .\scripts\env.ps1
cargo --version
pnpm --version
git --version
```

- 若 `cargo` 不存在 → 先跑 `scripts/install-toolchain.ps1`（幂等）。
- 若 `git` 访问远程失败并报 `SEC_E_NO_CREDENTIALS` → `env.ps1` 没有生效。
- **不要**用 `python` / `curl` / `Invoke-WebRequest` 访问网络：本机 schannel 凭据链损坏，它们会失败。改用 Node.js。

## 2. 状态核对

- [ ] `git status` 干净，或有明确属于「进行中」的改动（对应 STATUS.md 的 In Progress 条目）。
- [ ] STATUS.md 中**当前 Phase** 的条目与实际代码一致；不一致说明上次没收尾，先修 STATUS.md。
- [ ] 没有跨越人工检查点：当前 Phase 已达成的最后检查点是否已获用户确认？

## 3. 动手前的四问

1. **这属于哪个 Phase？** 不属于当前 Phase 的功能一律不写（写进 backlog，不改 STATUS 状态）。
2. **动到用户确认边界了吗？**（重大架构、不可逆迁移、覆盖文件、删数据、花钱、私钥、上传数据、改隐私策略、跳检查点、改统计口径）→ 先问用户。
3. **需要新增或修改 ADR 吗？** 会影响后续多个模块的决策 → 先写 ADR 草稿再写代码。
4. **验证手段是什么？** 写之前就明确「怎么证明它真的工作了」，并写进 STATUS.md 的备注。

## 4. 收尾（每次都必须做）

- [ ] `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `pnpm typecheck` + `pnpm test`（涉及 TS 时）
- [ ] `pnpm build` / `cargo build`（构建验证）
- [ ] 更新 STATUS.md 状态（**只写真实达到的状态**）
- [ ] 新增/更新 ADR（若有决策）
- [ ] 更新 `docs/development/platform-matrix.md`（若做了平台实测）
- [ ] 在 `STATUS.md` 的「已知问题 / 未解决问题」里登记遗留项

## 5. 禁止事项速查

| 禁止 | 原因 |
| --- | --- |
| 把「能编译」写成 `Tested` | STATUS.md 语义见 AGENTS.md §10 |
| 用静态假数据填充页面 | 规范 §24 明确禁止 |
| 在 Engine 里调用 `SystemTime::now()` / 随机数 / 数据库 | 规范 §4.2，会破坏 Golden Tests |
| 混用 SQLx 与 rusqlite | AGENTS.md §3 硬约束 |
| 默认上传任何活动数据或开启遥测 | AGENTS.md §8 |
| 编造性能数字 | 规范 §44.2 |
| 自行购买/注册付费账户、用用户私钥 | 规范 §46 |
| 未经确认跨越检查点 A / B / C | 规范 §0.2 |
| 在 .ps1 文件里写非 ASCII 字符 | 本机 PowerShell 5.1 + GBK 代码页会破坏脚本 |
