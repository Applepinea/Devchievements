# 第 34 章　Phase 2b：完整 Desktop Only 产品

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 34. Phase 2b：完整 Desktop Only 产品

## 34.1 实现内容

- Dashboard、Achievement Gallery（含新解锁标记）、Activity & Statistics、Repository Manager、About & Diagnostics。
- 历史导入界面与完整流程（包括安装向导中的导入步骤）。
- 基础 Settings：通知开关、音量与静音、隐私、运行模式显示、Git 身份、时区与日期、数据管理。
- 安装向导（桌面部分）、卸载机制（清理 Hook 或确保残留 Hook 静默退出）、数据备份与恢复。
- 平台能力探测、降级链与兼容性矩阵（对当前可用平台实测，其余标记未验证）。
- Desktop Only 下依赖浏览器能力的指标与成就显示「需要浏览器扩展」。

补充：存储健康诊断、Outbox 可观测性（延迟分布与原因码）、空状态与引导。

## 34.2 验收

页面均从真实数据库读取数据；新解锁标记覆盖实时、延迟、回填与静默解锁；历史导入端到端通过（含 Commit 时间归档与静默）；安装、升级、卸载流程通过；测试报告与 STATUS.md 更新。

---
