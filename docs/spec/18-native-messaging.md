# 第 18 章　Native Messaging 协议

> 本文件是 **Devchievements V2.5 规范** 的拆分件，正文与原文逐字一致，仅按章切分以便引用。
> 修改规范内容需经用户确认（见 `46-user-confirmation-boundary.md`）。

---

# 18. Native Messaging 协议

## 18.1 通信结构

```text
Browser Extension → Native Messaging Host → Desktop Companion → Core Engine → SQLite
```

Native Messaging Host 是浏览器按需拉起的短生命周期桥接进程，不是 Companion 本身。

## 18.2 数据权威

Full Experience 中 Companion 是唯一数据权威、唯一 SQLite 写入者、唯一成就仲裁者；浏览器扩展不得自行修改权威统计结果。

## 18.3 协议版本协商

握手必须支持 `protocol_version`、`min_supported_version`、`max_supported_version`、`capabilities`、`client_version`。不兼容时明确报错，不得继续把消息当作正常业务事件处理，不得静默忽略版本冲突。

## 18.4 安全性

校验扩展 ID（区分开发版与商店版）；验证消息结构、类型与大小上限；设置超时；防止错误来源提交事件；考虑 Snap、Flatpak 等 Linux 打包环境对 Native Messaging 的限制；禁止使用无保护的 localhost HTTP 替代 Native Messaging。

---
