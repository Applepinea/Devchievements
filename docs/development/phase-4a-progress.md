# Phase 4a 进展（自定义系统）

- **开始**：2026-11-17（用户以「进入 phase4」批准跨越检查点 C）
- **状态**：**In Progress**

---

## 1. 为什么先做校验层而不是界面

4a 的界面（Sound Manager、Asset Manager、Popup Editor）都在「导入用户资源」
这个前提下工作，而**导入是不可信输入的入口**。

先做界面后做校验，意味着有一段时间产品能接受任意文件。先做校验，
界面接上去时就已经受约束。

## 2. 资源安全校验（4a.1）

`crates/assets`，14 个测试。

### 2.1 为什么扩展名不够

一个叫 `icon.png` 的文件可以是带 `<script>` 的 HTML 文档。
扩展名是**用户对自己文件的声明**，magic bytes 才是**关于它的事实**。
只有后者可以依赖。

### 2.2 SVG 单独处理

SVG 是文档格式，可以携带脚本、事件处理器和外部引用。
按 §20 只允许「安全处理过」的 SVG，所以采用**保守白名单**：

| 拒绝的构造 | 原因 |
| --- | --- |
| `<script` / `<foreignObject` / `<iframe` | 可执行或嵌入文档 |
| `onload` / `onerror` / `onclick` | 事件处理器 |
| `href="http` / `xlink:href` | 外部引用 |
| `javascript:` | 直接执行 |
| `<!ENTITY` | XXE |

**刻意选择拒绝而不是清洗**：一个微妙错误的清洗器是「失败时放行」，
而拒绝只需多试一个文件。

非 UTF-8 的 SVG 也直接拒绝——有损转换可能掩盖构造。

## 3. Popup Editor 的约束层（4a.2）

`crates/themes`，15 个测试。

§21 给了明确范围（宽度 280–600、缩放 70–150% 等）。
**滑块能表达任何数字，所以范围必须落在可测的地方**，
否则一个导入的主题把宽度设成 4000，弹窗就盖住整屏。

`PopupStyle::clamped()` 在**读入时**执行，不是在使用时。
定位也写成纯函数 `position(screen)`，四角算术不需要真实显示器就能测。

## 4. 主题包（4a.3）

按 §23「主题包不得包含可执行代码」。做法是**扫描整个文档的键名**，
遇到 `script` / `css` / `html` / `javascript:` 直接拒绝。

### 一个顺序 bug

最初先反序列化再检查版本，结果一个「未来 schema 且字段不全」的主题报的是
「缺字段」而不是「版本不支持」——**用户会去找缺失的字段，而真正的问题是版本**。

改成先查版本再解析。这个问题只有在写「未来版本」测试时才会暴露，
而那条测试我一开始没想到要写。

## 5. 配置导入导出（4a.9）

`crates/config`，11 个测试；`scripts/e2e/config-io.mjs`，18 项端到端。

### 5.1 导出是承诺，不是私有状态

配置文件和应用自己的 `settings.json` 分开：
导出物要能搬到另一台机器，所以它**声明版本**、**pretty-print**、可读可 diff。

### 5.2 一个顺序问题

最初直接反序列化，结果 `volume: 300` 报的是
“invalid value: integer 300, expected u8”——**用户看不出该怎么办**。

改成先钳制数值再类型化解析：手改的文件仍然能载入，值被拉到合法范围。
但**语义上不可能的仍然拒绝**：登录名为空的映射被拒，因为那会改变含义。

### 5.3 失败必须不改变现状

e2e 里专门有一条：导入一个未来 schema 的文件 → 拒绝 →
**检查原有配置一个字节都没变**。「导入了一半」比「完全没导入」更糟。

## 6. 弹窗应用外观设置

弹窗原来用写死的 364×88。现在读 `PopupStyle`，
e2e 把宽度设成 520、角落设成左上，断言日志出现 `width=520 corner=top_left`。

顺带把日志的角落名从 Debug 的 `TopLeft` 改成 `top_left`：
日志是给人看和给测试读的，两种拼写只会制造无谓的差异。

## 7. 音效覆盖

e2e 5 项：无覆盖 → 覆盖后不同 → 覆盖值正确 → 静音仍然优先。

### 7.1 三次选错槽位

| 我用的槽位 | 结果 | 原因 |
| --- | --- | --- |
| `global` | 不生效 | 稀有度的内置音效排在全局自定义之前（刻意的判断） |
| `rarity:common` | 不生效 | 第二次解锁的是 Uncommon Day |
| `rarity:uncommon` | 不生效 | 该成就 id 是 `daily:uncommon`，走 daily 分支，daily 排在 rarity 之前 |
| `daily:uncommon` | ✅ | 正确 |

**三次失败都是「测试写错了」，不是产品坏了。**
但它们合起来揭示了优先级链的真实形状——而我一开始只是照着印象猜槽位。

教训：写这种测试时，先看**实际会走哪条分支**（日志里已经有 `[Uncommon]` 和 id），
而不是照规范条文推一个「应该生效」的槽位。

另有一处夹具缺陷：git 子进程没有继承 `LOCALAPPDATA`，
helper 因此判定 companion 不存在而静默退出（那是 §34.1 要求的正确行为）。
**产品行为对，测试夹具错。**

## 8. Achievement Builder 的 JSON 层（4a.8）

`crates/achievements/src/definition.rs`，12 个测试；
`scripts/e2e/achievement-builder.mjs`，12 项端到端。

### 8.1 四条拒绝规则

| 规则 | 为什么 |
| --- | --- |
| id 必须是 slug | 它会被写进存储和 URL，自由文本是隐患 |
| **不得遮蔽内置成就** | §13.3 要求内置口径固定；覆盖会静默改变用户可能已解锁的结果 |
| 不得与已有用户成就重名 | 否则「哪一个是我的」无法回答 |
| 复杂度超限拒绝 | §22 的深度/条件数/步数上限 |

第一条和第三条在拒绝时都**不写入任何东西**——e2e 最后一条专门验证「被拒的定义一个都没留下」。

### 8.2 一个 JSON 形状陷阱

我按直觉把规则写成嵌套的：

````json`
{ "op": "condition", "Condition": { "metric": "commits", "at_least": 10 } }
`````

实际 serde 的内部标签枚举把 newtype variant 的字段**内联**了：

````json`
{ "op": "condition", "metric": "commits", "at_least": 10 }
`````

报错是「missing field metric」——不会提「你多套了一层」。

修法是从**实际序列化输出**里读出真实形状，而不是照文档推。
并且加了一条测试 `the_wire_shape_is_flat_for_a_condition` 把它钉住：
这个形状是用户要手写的，写错在文档或夹具里是静默陷阱。

### 8.3 加字段不能破坏旧导出

新增 `userAchievements` 后，三个 config 测试立刻失败——
它们的夹具没有这个字段，而 serde 要求它。

加了 `#[serde(default)]`。
**导出是承诺**：加一个字段不应该让用户手里已有的文件失效。

## 9. 界面层（4a.6 / 4a.7）

### 9.1 Event Filters 界面

设置页可以：排除仓库（增删）、开关「计入合并提交」「计入导入历史」。

`scripts/e2e/filters.mjs` 9 项，其中最关键的一条：

```text
ok  the official repository list is unchanged
```

**排除一个仓库，官方统计一个字节都不变**（§13.3）。
这条测试守着整个 filters crate 存在的理由——如果它红了，说明用户
「为了看干净点的面板」而永久失去了某条成就，那是最难解释的一类 bug。

### 9.2 Popup Editor

宽度、缩放、角落、边距四个可调项，加一个 **Preview popup** 按钮。

预览会**先保存再显示**：
一个展示未保存值的预览是在撒谎——用户看到的不是下一条通知的样子。

### 9.3 一处需要两端的类型

加 popup 到设置视图时，Rust 侧和 TypeScript 侧各要声明一次。
这是 Tauri 的固有代价：**契约在两个语言里各写一遍**，
漏一边的表现是「编译通过、运行时字段 undefined」。
缓解办法是让 Rust 的结构体字段顺序、命名和 TS 接口逐字对应，
并靠 e2e 在真实窗口里读字段来兜底。

### 9.4 仍然是 Partially Implemented 的三项

| 项 | 缺什么 |
| --- | --- |
| Sound Manager | 没有独立的音效管理页；要换音效得手改配置或用导入 |
| Asset Manager | 校验层完备，但没有「选文件 → 导入 → 预览」的界面 |
| Popup Editor | 有基本调节与预览，缺透明度、动画速度、多显示器选择 |

这三项都依赖 Tauri 的文件对话框插件，属于新增依赖，我没有擅自加。

## 10. 资产导入（4a.5）

引入 `tauri-plugin-dialog`（经用户批准）。设置页现在可以「导入文件…」，
但**对话框只负责给出路径**——决定收不收的是字节。

### 10.1 端到端 9 项

```text
ok  a real PNG is accepted
ok  it was stored
ok  a renamed script document is refused
ok  the refusal says why
ok  it was not stored          <-- 拒绝必须意味着没写进去
ok  a plain SVG is accepted
ok  an SVG containing a script is refused
ok  the refusal names the construct
ok  only the accepted files are present
```

第 5 条和第 9 条是重点：**被拒的文件不能在磁盘上留下痕迹**。
只断言「返回了错误」是不够的——错误路径里可能先写了再检查。

### 10.2 新增依赖的代价

| 项 | 变化 |
| --- | --- |
| `Cargo.toml` | 新增 `tauri-plugin-dialog` |
| `capabilities/default.json` | 新增 `dialog:allow-open` |
| 前端 | 直接调用 `plugin:dialog|open` 命令，不引入 JS 包 |

capability 文件此前不存在，是这次新建的。**权限声明从此成为需要维护的东西**：
以后任何新的 plugin 命令都要在这里加，否则调用会静默失败。

### 10.3 仍然没做

- 音效的独立管理页（换音效仍要改配置）
- 资产在弹窗/图鉴里的实际使用（导入的文件目前只是存下来）
- 裁剪、缩放、恢复默认

第二点尤其要说明：**现在导入的资产还没有被任何渲染路径使用**。
这一步只完成「能安全地把它收进来」，不等于「它已经生效」。

## 11. 资产接入渲染（4a.5 完成）

之前只能导入，导入的文件没有被任何地方使用。现在接上了。

### 11.1 命名约定而不是映射表

一个叫 `achievement-<id>.<ext>` 的文件覆盖该成就的图标。
**用约定而不是映射表**：映射表要持久化、要处理重命名、要在删除时清理；
约定是无状态的，删文件即失效。

id 必须是 slug 才能匹配（slug 不含路径分隔符），这也顺带堵住了路径穿越。

### 11.2 解析放在 Rust，不放在页面

页面拿到的是 `icon_url`，不是「去哪个目录找」。原因：

- 页面不能探测文件系统；
- 找不到文件时要**静默降级成字母**，而不是显示坏图标。

`asset://localhost/...` 通过 Tauri 的 asset 协议加载，scope 限定在 assets 目录，
**不是整个磁盘**。

### 11.3 三个权限陷阱

| 现象 | 真因 |
| --- | --- |
| 构建时报「features 与 allowlist 不匹配」 | 启用 asset 协议必须在 Cargo.toml 加 `protocol-asset` feature |
| 报「Permission core:asset:default not found」 | 该权限在这个 Tauri 版本**不存在**；asset 协议靠 conf 的 scope 而非 capability |
| 报 missing field `icon_url` | 有两个 `PopupPayload` 初始化点，只改了一个 |

第三条是这一轮重复出现的模式：**同一个结构体有多个构造点**。
前几轮是 `author_login`、`merged`，这次是 `icon_url`。
编译器这次抓到了（struct literal 要求全部字段），但如果构造点用的是 `..Default::default()` 就不会。

### 11.4 验证

`popup.mjs` 新增一条：导入一个图标后，弹窗日志必须出现 `icon_url`。
这连着两件事：**导入成功** 和 **它真的被用上了**。

只测「导入成功」是不够的——那正是上一轮的状态，功能存在但没生效。

## 12. Popup Editor 补完（4a.6）

§21 的属性表现在全部可调：

| 属性 | 范围 |
| --- | --- |
| 宽度 | 280–600 |
| 缩放 | 70–150% |
| 图标尺寸 | 32–160 |
| 圆角 | 0–32 |
| 屏幕边距 | 0–100 |
| 位置 | 四角 |
| 透明度 | 20–100% |
| 显示时长 | 1–30 秒 |
| 动画速度 | 0–1000 ms |
| 背景模糊 | 0–24 px |

动画和模糊此前**完全不存在**，这次新增并带默认值（240ms / 0px），
所以旧配置与旧主题包仍然能载入。

### 12.1 显示时长必须真的控制寿命

设置里改成 9 秒，弹窗却仍然 5 秒后消失——那是最容易漏的一环：
值存下来了、界面也显示对了，只是没人用它。

现在 `show()` 读 `style.display_seconds` 决定存活时间，
常量只作兜底。

### 12.2 三个构造点，编译器全抓到了

加 `style` 字段后有 3 处 `PopupPayload` 初始化报 missing field。

这次没漏，因为用的是 struct literal（要求列出全部字段）。
**但这也是运气**：如果哪一处用了 `..Default::default()`，就静默通过，
而那正是前几轮 `author_login`、`merged`、`icon_url` 出问题的方式。

反复出现的模式值得记下来：**一个结构体被多处构造时，加字段是高风险操作**。

### 12.3 验证

`popup.mjs` 现在把每个字段都设成非默认值，再断言它们出现在弹窗日志里：

```text
ok  the configured style was applied
ok  the render values travel with the payload
```

**全部设成非默认值**是关键：如果某项沿用默认，而应用恰好忘了读它，
测试会因为「默认值 == 期望值」而通过。

## 13. Sound Manager（4a.4 完成）

### 13.1 用户音效与内置音效分开存放

内置音效在可执行文件旁边，用户导入的放在数据目录的 `assets/`。
分开放的理由很实际：**更新会替换内置音效集**，而用户文件必须活下来。

查找顺序是用户目录优先——导入本身就是一次明确的选择。

### 13.2 一个容易忽略的降级要求

§19 要求「资源缺失时安全回退」。
所以删掉用户音效后，行为必须是**退回内置**，而不是变安静。

e2e 里两条相邻的断言守着这件事：

```text
ok  it falls back to a built-in sound
ok  it does not go silent
```

只测第一条是不够的：一条完全静音的路径也能让「没播用户音效」成立。

### 13.3 又一次选错槽位

我最初把覆盖设在 `daily:uncommon`，但 `First Commit` 是 rarity common、
且不是 daily 奖励，所以那条槽位根本不参与。

改成 `rarity:common` 后通过。
这是本阶段**第三次**在同一处犯同类错误（前两次在 sound-overrides）。

根因不是粗心，是我一直照规范条文推「哪个槽位应该生效」，
而没有先看**这次实际会走哪条分支**。
日志里其实一直有答案（`[Common]`、成就 id），我没先读它。

### 13.4 现在 4a 的状态

九项里九项都是 `Tested` 或已完成。
Sound Manager 的「独立管理页」也接上了：设置页可以导入音效、列出可用音效。
