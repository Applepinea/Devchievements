# Devchievements

Achievements for Developers — 一个本地优先的开发者成就系统。

Devchievements 观察你本机的 Git 活动（以及浏览器扩展上报的 GitHub 活动），把它变成**永久成就**、
**每日等级**和**连续活跃记录**，用弹窗和提示音庆祝。所有数据默认只存在本机。

> 独立第三方软件，与 GitHub 官方没有任何隶属、认可或赞助关系。

---

## 功能

### 永久成就（解锁一次，永不撤销）

内置目录当前有 **9 个**成就，全部按官方统计口径判定，且**累计类成就是跨仓库合计**
（Century 是「任何地方累计 100 次」，不是「某个仓库 100 次」）：

| 成就 | 条件 | 稀有度 |
| --- | --- | --- |
| First Commit | 第 1 次有效提交 | Common |
| Getting Started | 累计 10 次有效提交 | Common |
| Century | 累计 100 次有效提交 | Rare |
| Thousand Commits | 累计 1000 次有效提交 | Legendary |
| First Release | 发布第 1 个 Release | Uncommon |
| Merged | 第 1 个 PR 被合并 | Uncommon |
| One Week | 连续 7 天活跃 | Rare |
| One Month | 连续 30 天活跃 | Epic |
| A Year of Code（隐藏） | 连续 365 天活跃 | Legendary |

**关于连续天数**：`One Week` / `One Month` / `A Year of Code` 的规则已存在并有测试，
但评估快照里的连续天数字段目前恒为 0，所以它们**还不会真正解锁**。这是已知缺口，
见 [STATUS.md](STATUS.md) 的「已知功能缺口」一节。

### 每日等级

按**当天所有仓库合计**的有效提交次数给一个等级（三个仓库各 3 / 2 / 1 次就是当天 6 次，不是三次独立的小额），
一天最多升一次，升级时弹窗：

| 次数 | 等级 |
| --- | --- |
| 0 | 无 |
| 1–3 | Common |
| 4–9 | Uncommon |
| 10–19 | Rare |
| 20+ | Legendary |

### 通知

- 普通提交只播一声轻提示音，**不弹窗**；成就解锁与等级升级才弹窗。
- 同一事件触发多项奖励时合并成一个弹窗（主标题取最高稀有度）。
- 事件按年龄分档：10 分钟内正常通知，10 分钟–24 小时**汇总为一条**（不弹窗），
  超过 24 小时只静默更新统计。
- 弹窗走应用自己的窗口，不使用 Windows 系统通知（见 ADR-0015）。

### 自定义

- **主题**：保存/切换/导入/导出/恢复默认的弹窗外观；主题包不得包含任何代码，导入时校验。
- **Popup Editor**：宽高、缩放、角落、边距、圆角、图标尺寸、透明度、动画、模糊、显示时长。
- **资源与音效**：导入图片（读魔数校验真实类型）与 WAV 音效；**每个槽位可试听**，
  试听走真实选择路径（用你自己的覆盖配置与音量），静音时会告诉你「选中了什么、为什么没播」。
- **事件过滤器**：按仓库/作者/时间范围筛选用户口径的统计。
- **自定义成就**：写一条规则（JSON 或表单），在满足它的那次提交上解锁。
- **历史导入**：导入已有仓库的历史提交，静默入库、幂等、可续传。

---

## 支持范围

**只支持 Windows 10 / 11。** macOS 与 Linux 不在支持范围内，这是产品决定而不是遗漏，
理由见 [ADR-0017](docs/decisions/0017-windows-only.md)。

---

## 安装

下载安装包（`Devchievements.msix` 或 `Devchievements_0.1.0_x64-setup.exe`）并运行。安装器会检查 WebView2 运行库并在缺失时安装它。

**未做**：安装包没有 Authenticode 代码签名（没有证书），SmartScreen 会提示；更新签名的 `.sig`
是另一回事，两者不能互相替代。

---

## 构建

环境准备（每个新 Shell 都要做一次）：

```powershell
. .\scripts\env.ps1
```

桌面应用是**独立**的 cargo 工程（MSVC 工具链），其余 crate 走 workspace（GNU 工具链）：

```powershell
# 桌面应用（前端 + Rust）
cmd /c "scripts\dev\build-desktop.bat build"

# 安装包（NSIS）
cmd /c "scripts\dev\build-installer.bat"

# 浏览器扩展
pnpm --filter @devchievements/extension build
```

发布构建要用 `DEVCH_RELEASE=1`，并提前在环境里给出 `TAURI_SIGNING_PRIVATE_KEY` 与
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`；缺任何一个都会直接失败，不会回退到仓库里的开发密钥。

---

## 测试

一条命令验证整个仓库：

```powershell
node scripts/verify.mjs
```

它依次跑 fmt / clippy / test / build / build-wasm / 桌面端类型检查 / 桌面端前端构建 /
桌面端 Rust 构建 / 扩展构建 / 扩展单元测试，日志写入 `.verify/`。

当前实测（2026-11-19 23:42）：**10 个步骤全部 PASS，483 个 Rust 测试通过**（另有 3 个 ignored）。

另有真实的端到端脚本（真二进制、真数据库、真窗口），见 `scripts/e2e/`，例如：

```powershell
node scripts/e2e/full-loop.mjs          # git commit → hook → helper → spool → companion → sqlite → 弹窗
node scripts/e2e/ui-pages.mjs           # 真实窗口逐页打开
node scripts/e2e/csp.mjs                # 内联脚本注入被 CSP 拒绝
node scripts/e2e/theme-manager.mjs      # 主题导入/应用/导出/恢复默认
node scripts/e2e/builder-form.mjs       # 自定义成就真实解锁
```

**未验证**：CI 从未在 GitHub Actions 上运行过（本机没有仓库）；Authenticode 签名无证书。

---

## 隐私

- **默认全部本地。** 不上传 Git 历史、提交信息、仓库名、PR 标题或任何活动记录。
- **没有遥测**，也没有开关可以打开它。
- 诊断导出默认脱敏（仓库与路径别名化）；只有显式勾选才包含真实名称，且警告写在文件本身里。
- **Privacy Mode**：隐藏仓库名与详情，用通用文案，并同样作用于日志与诊断。
- Privacy Mode **不是加密**，不要当成加密。

---

## 文档

- [STATUS.md](STATUS.md) — 唯一权威进度表，含每个状态对应的复现命令
- [AGENTS.md](AGENTS.md) — 本仓库的强制工程约束
- [docs/decisions/](docs/decisions/) — 架构决策记录（ADR）
- [docs/spec/](docs/spec/) — 产品规范拆分件

---

## 许可证

GNU General Public License v3.0 or later（GPL-3.0-or-later）。全文见 [LICENSE](LICENSE)。
