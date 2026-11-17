# Phase 3a 进展（Browser Only）

- **开始**：2026-11-17（用户以「继续 phase3」批准跨越检查点 B）
- **状态**：**In Progress**

---

## 1. 扩展终于能被加载了

### 1.1 先纠正一个旧结论

ADR-0001 曾判定「MV3 浏览器内加载在本机无法验证」，**这个结论是错的**。

两个原因叠在一起：

1. 当时只试了 headless，而 headless **不支持加载扩展**
   （实测：`Multiple targets are not supported in headless mode`）。
2. 有窗口时 `--load-extension` 在当前 Chromium 上也已被忽略——
   而「被忽略」和「加载失败」从外面看完全一样。

正确做法：CDP 的 `Extensions.loadUnpacked`（需 `--enable-unsafe-extension-debugging`），
再用 `Target.attachToTarget` 连进 MV3 service worker 求值。

### 1.2 顺带发现的真 bug

扩展**从来没有被加载过**：清单里手写了 `content-scripts/github.js`，
但 WXT 不会因为手写清单就生成这个文件，浏览器直接拒绝安装。

改用 WXT 入口约定 `entrypoints/github.content.ts`。

## 2. 活动识别（3a.3）

设计要点：**把「是不是可验证活动」写成纯函数**，页面由一个小接口描述：

```ts
interface PageFacts {
  url: string;
  meta(name: string): string | null;
  text(selector: string): string | null;
}
```

内容脚本用 `document` 实现它，测试用固定数据实现它。
于是规则可测，DOM 只剩十行适配。

### 只认可验证的事实

- 仓库来自 URL 路径；保留顶层路径（`/settings`、`/notifications`）不算仓库
- 提交 id 来自 URL，要求 7–40 位十六进制（GitHub 不会更短）
- **时间必须来自页面给的机器可读值**；没有就忽略，不猜

「没有日期就忽略」这条是刻意的：猜一个日期会让事件落到错误的一天，
而错误的数据比缺失的数据更难发现。

### 去重键

提交：`repo#sha`；发布：`repo#release:<tag>`。
事件 id 由去重键派生，所以同一页面重复上报不会重复计数。

9 个测试覆盖：正常提交、幂等、大小写归一、缺日期、发布、两个发布不同、
非活动页面、保留路径、`.git` 后缀。

## 3. 持久化（3a.4）

| 层 | 内容 |
| --- | --- |
| `EventStore` 接口 | `add`（返回是否新增）、`all`、`counts` |
| 内存实现 | 单元测试用，5 个测试 |
| IndexedDB 实现 | 浏览器用，端到端验证 |

IndexedDB 里 `add` 撞主键就是「已经记录过」，不是错误——
用 `preventDefault` 把它转成 `false` 返回值。

## 4. 端到端证据

NaN 现在 12 项：

```text
ok  the extension installs
ok  the MV3 service worker is running
ok  it is our extension
ok  the Rust core is packaged
ok  it instantiates inside the worker under the MV3 CSP
ok  a commit page is recorded
ok  the same commit is not recorded twice
ok  an unrelated page is ignored
ok  the worker reports one stored event
ok  it is counted as a commit
```

### 一个必须写下来的坑

MV3 worker 刚被唤醒时，**第一条消息会丢**（监听器还没注册）。
测试第一次跑就因为这条误判成「事件没记录」。

修法不是加重试掩盖问题，而是先做一次 stats 往返握手，
确认通道活着再开始断言。

## 5. 仍未做

- Outbox 队列（离线重放、原因码）
- 浏览器通知与音效
- 浏览器端设置/Dashboard/图鉴
- 身份映射 GitHub Login → Git Email
- **Service Worker 回收后的恢复没有实测**：数据在 IndexedDB 里，
  重启后理论上还在，但「强制停掉 worker 再读」这一步没做过，所以不算验证。

## 6. 浏览器 Dashboard（3a.6 部分）

弹窗现在是真界面：统计、最近活动、空状态，全部来自 service worker 读 IndexedDB。

### 三个断言

```text
ok  the dashboard lists the recorded commit
ok  it shows the repository
ok  the commit count is rendered
```

### 两个教训

**1. 我把挂载入口覆盖了。**

重写弹窗时，我把 `entrypoints/popup/main.tsx` 整个换成了组件，
连 `createRoot` 一起删掉了。构建和类型检查全过，页面却是一片空白——
因为**没有任何东西会去挂载组件**。

诊断靠的是读 `document.getElementById('root').childElementCount`：
页面加载完成、脚本和样式都在、root 是 0。这比「白屏」三个字有用得多。

现在 `main.tsx` 只做挂载，`App.tsx` 才是界面。

**2. 数据是测试先写、页面后开的。**

弹窗在事件写入**之前**就打开了，读到的是空状态。
测试里刷新一次再读，顺带证明了数据是**从 IndexedDB 读回来的**，不是页面内存里的残留。

## 7. 扩展 → 桌面：真实握手（3b.1–3b.4）

### 7.1 链路

```text
GitHub 页面 → 内容脚本描述页面 → service worker 提取事件
   ├─ 存本地 IndexedDB（供 Dashboard 显示，不做判定）
   └─ connectNative → host 校验 → Spool → Companion 判定 → SQLite → 弹窗
```

### 7.2 三个必须同时成立的部件

| 部件 | 为什么不能少 |
| --- | --- |
| 提交的公钥（`extension-key.pub`） | 扩展 ID 由它推导；否则 ID 随安装路径变化，host 的 `allowed_origins` 永远对不上 |
| `nativeMessaging` 权限 | 没有它 `chrome.runtime.connectNative` **根本不存在** |
| HKCU 注册表项 | 浏览器靠它找到 host 清单 |

### 7.3 抓到的两个真 bug

**1. 浮窗 Dashboard 在转发模式下变空。**

最初实现是「先转发，成功就不落地」，结果浏览器里什么都看不到——
因为判定和存储都在桌面端。
改成**两边都记**：本地留痕供界面显示，判定仍归桌面。
本地存储是缓存，不是第二套规则。

**2. 清单缺 `nativeMessaging` 权限。**

握手直接报 `connectNative is not a function`。
这类错误很隐蔽：代码看起来完全正常，只是那个函数根本不存在。

### 7.4 证据

`extension-load` 现在 **18 项**，包含真实握手：

```text
ok  the extension id matches the committed key
ok  the browser can launch the native host
ok  the host acknowledges the report
```

加上 `native-host` 的 11 项（含 companion 入库断言），
以及 `companion_loop` 里那条浏览器事件必须被判定为提交的测试。

## 8. 最后一环：浏览器报告触发桌面弹窗（3b.5）

### 为什么必须端到端测

前面的部件各自都有测试：

| 部件 | 已有证据 |
| --- | --- |
| host 解析与落盘 | `native-host` 11 项 |
| companion 判定浏览器事件 | `companion_loop` 1 项 |
| 弹窗呈现 | `popup` 8 项 |
| 浏览器真实握手 | `extension-load` 18 项 |

但**它们能否串起来从没验证过**。断在「host 写了记录」与「companion 展示了它」之间的任何位置，
上面每一项都仍然是绿的。

`scripts/e2e/browser-to-popup.mjs` 补的正是这个空档：

```text
ok  the host accepted the browser report
ok  the browser event reached the database
ok  the desktop app opened a popup for it
ok  the popup was not a failed creation
ok  no system notification was attempted
```

**注意最后一条**：它同时守住了「不再走系统通知」这个决定（ADR-0015）。
如果谁把通知路径加回去，这条会变红。

### 清理

注册脚本动的是真实的 `HKCU` 和 `%LOCALAPPDATA%`，
所以补了 `unregister-native-host.mjs`。
**一个测试如果在机器上留下痕迹却不清理，它就是在静默地改变环境。**

## 9. Full Experience 的边界（诚实记录）

现在能做的：

- 扩展检测 GitHub 提交/发布 → 本地 host → 桌面判定 → 桌面弹窗。
- 扩展同时保留本地副本，供浏览器内 Dashboard 显示。

**不能做的**：

- 没有桌面应用时，扩展**不解锁任何成就**。这是 ADR-0018 的自觉选择，
  不是缺陷：判定规则只有一份，在 Rust 里。
- 浏览器内没有设置页、没有图鉴。

## 10. Service Worker 回收恢复（3a.8）

### 为什么这条一直标着「未实测」

数据在 IndexedDB 里，重启后理论上还在。但**「理论上」不是证据**：
如果读回来的东西恰好来自模块作用域，测试也会绿，而生产里 worker 一被杀就丢。

### 怎么才算是真的测了

必须让 worker **真的死掉**，再通过一个新 worker 读。
所以脚本里有一条专门断言「worker 确实停了」：

```text
ok  our worker is running before the stop
ok  the worker was actually stopped      <-- 关键
ok  the restarted worker answers
ok  the event survived the restart
ok  the event details survived too
```

**没有第二条，后面几条可能只是因为 worker 压根没停过**，
读到的还是同一块内存。这正是「看起来测了其实没测」的典型形态。

### 过程中的一处 API 误用

`ServiceWorker.stopAllWorkers` 不在浏览器级会话上，只在页面级会话上。
改用 `Target.closeTarget` 关掉 worker 目标——
效果相同（worker 没了），而且不依赖会话类型。

## 11. 仓库身份映射（3a.7）

### 这是个真漏洞，不是待办

之前：

| 路径 | 记录的仓库 id |
| --- | --- |
| 本地 hook | `local:C:/repos/foo/.git` |
| 浏览器扩展 | `github:owner/foo` |

同一个仓库被记成**两个**。用户本地提交一次、网页上提交一次，统计里会出现两个仓库。
这违反 §6.1，而且是那种「不报错但数字全错」的问题。

### 修法

`crates/events/src/repository.rs` 里本来就有完整的规范化实现（SSH/HTTPS、`.git` 后缀、大小写、优先级），
**只是钩子路径从来没用过它**：它压根没读 remote。

改动：

1. helper 读 `remote.origin.url`（没有 origin 就退到任意 remote），写进 payload；
2. companion 用 `RepositoryIdentity::from_remote_url` 推导 canonical id；
3. 没有 remote 时才回退到 `local:<path>`。

### 两条测试，方向相反

| 测试 | 断言 |
| --- | --- |
| `hook_and_browser_agree_on_one_repository` | SSH 写法的 hook 与 HTTPS 写法的 browser **合成一个仓库**，2 个提交 |
| `a_repository_without_a_remote_stays_local` | 没有 remote 时**保持** local，不瞎合并 |

第二条和第一条同样重要。**过度合并比不合并更难发现**：
两个不相干的本地仓库被合成一个，数字看起来还很正常。
§6.1 明确禁止「轻率合并同名仓库」，这条测试就是守那条。
