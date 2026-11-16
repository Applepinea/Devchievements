# 第 40 章　Phase 4b：产品化与发布准备

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 40. Phase 4b：产品化与发布准备

## 40.1 产品化

UI 优化；多语言支持；性能优化；错误恢复；安装、升级与卸载体验；数据迁移；兼容性测试；完整隐私设置。

## 40.2 CI/CD

在 Phase 0 的基础 CI 之上扩展为完整发布流水线（GitHub Actions）：Windows、macOS、Linux 构建；按实际支持范围构建 x64 与 ARM64；Rust、TypeScript、WASM（如采用）、Extension、Desktop 构建；格式与静态检查；单元测试与 Golden Tests；数据库迁移测试；多 Git 版本的 Hook 实验矩阵。

## 40.3 更新机制

使用 Tauri Updater。必须考虑：完整性验证与签名；失败回滚；数据库与配置迁移；更新中断恢复；更新失败不得破坏现有数据库。

## 40.4 平台签名

Windows 代码签名；macOS Developer ID 签名与公证。不得假设未签名程序可以获得完整平台信任；具体证书类型和平台要求在发布前重新核实。

---
