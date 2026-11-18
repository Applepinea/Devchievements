# Phase 4b 进展（产品化与发布准备）

- **开始**：2026-11-17（用户以「继续做phase4b」批准进入）
- **状态**：**In Progress**

---

## 1. 安装包（§40.1 安装体验）

Phase 4b 的第一件事是让「交付物」真实存在：一个能装、能跑、能卸的 Windows 安装包。
之前 `cargo build --release` 已经产出 exe，但 `tauri bundle` 在打包阶段失败，
所以「安装包」这件事一直停在「应该可以」。

### 1.1 失败点与误判

bundler 的输出是：

```text
Info extracting NSIS
Error failed to bundle project: 拒绝访问。 (os error 5)
```

第一反应是「工具缓存目录不可写」，于是把 `TAURI_BUNDLER_TOOLS_DIR` 指向工作区。
**这个变量在 tauri-bundler 2.10.5 里根本没人读**——查源码（
`bundle/windows/util.rs::tools_directory`）才确认，缓存位置只有两种来源：

| 来源 | 路径 |
| --- | --- |
| `bundle.useLocalToolsDir: true` | `<local tools dir>/.tauri` |
| 默认 | `<cache dir>/tauri`，即 `%LOCALAPPDATA%\tauri` |

所以那次改动只是把问题藏起来，没有解决问题。

### 1.2 真正的做法

bundler 的行为是：先看 `<tools>/NSIS` 是否**已经完整**，完整就完全跳过解压。
它按一份硬编码清单校验（`makensis.exe`、`NSIS.exe`、`Bin/makensis.exe`、
`Stubs/lzma*-x86-unicode`、若干 `Include/*.nsh`、以及插件的 `nsis_tauri_utils.dll`），
并额外校验 `NSIS.exe` 的 SHA1。

于是把工具集**按它要的样子**摆好，解压这一步就不会被执行：

- `scripts/dev/prepare-nsis.ps1`：把 `_toolchain/tauri-tools/nsis/nsis-3.13` 复制成
  `%LOCALAPPDATA%\tauri\NSIS`，再把 `nsis_tauri_utils.dll`（v0.5.3，SHA1 校验一致）
  放进 `Plugins/x86-unicode/additional/`；清单缺一项就报错退出，不静默继续。
- `scripts/dev/build-installer.bat` 在打包前调用它，并删掉了那条无效的
  `TAURI_BUNDLER_TOOLS_DIR`。

结果：

```text
Running makensis to produce ...bundle\nsis\Devchievements_0.1.0_x64-setup.exe
Finished 1 bundle at: ...Devchievements_0.1.0_x64-setup.exe (3.64 MiB)
```

### 1.3 那条 `__TAURI_BUNDLE_TYPE` 警告是良性的

重复打包时会看到：

```text
Warn Failed to add bundler type to the binary: __TAURI_BUNDLE_TYPE variable not found
```

原因是 `tauri-utils` 里放的是 `static mut __TAURI_BUNDLE_TYPE = "__TAURI_BUNDLE_TYPE_VAR_UNK";`，
bundler 把这个占位串**原地改写**成 `..._VAR_NSS`。已经改过的二进制里当然没有 `..._UNK` 了，
于是第二次打包必然报这一句。检查安装后的二进制，`__TAURI_BUNDLE_TYPE_VAR_NSS` 是在的，
所以 updater 拿得到 bundle 类型，不需要为此改版本。

### 1.4 真机验证

`scripts/e2e/installer.mjs`（17 项检查，全部通过）：

| 检查 | 方式 |
| --- | --- |
| 安装包带更新签名 | `.sig` 存在 |
| **签名属于配置里的那把密钥** | 比对 `.sig` 与 `plugins.updater.pubkey` 的 8 字节 key id（`50de5a1a6e883d58`） |
| 静默安装成功 | `setup.exe /S /NS /D=<scratch>` |
| 主程序、helper、卸载器都落地 | 文件存在性 |
| 装好的程序能启动并自报状态 | `--diagnostics` 退出码 + JSON |
| 数据目录在安装目录之外 | 检查 `companion.db` 的位置 |
| 装好的程序如实报告更新状态 | `--update-status` 的 `configured === false` |
| 卸载成功并删掉程序 | `uninstall.exe /S _?=<dir>` |
| **卸载不删用户数据** | 卸载后 `companion.db` 仍在 |

「签名属于这把密钥」这条值得单独说：`.sig` 文件存在**不等于**它配得上这个安装包。
用另一对密钥签出来的文件长得一模一样，直到用户点更新才失败。
两边的 key id 都取第 2–9 字节，比一下就知道是不是一对——不需要实现签名验证就能挡住这个错误。

`/NS` 是必须的：静默安装默认会创建桌面和开始菜单快捷方式。
`/D=` 必须是**最后一个、不带引号**的参数，否则 NSIS 不认。

### 1.5 已知限制（必须在发布前处理）

`installMode: currentUser` 让安装目录等于 `%LOCALAPPDATA%\Devchievements`，
而数据目录是 `%LOCALAPPDATA%\devchievements`——**Windows 不区分大小写，两者是同一个目录**。
也就是说默认安装会把 exe 放进用户数据目录里。

已验证这不造成数据丢失：卸载器只删它自己写下的文件（模板里没有 `RMDir /r`），
最后一句 `RMDir "$INSTDIR"` 在目录非空时直接失败，所以数据库和配置留下。
但它仍然是「安装体验」上的缺陷：`打开数据文件夹` 会看到 exe，升级路径也不干净。

发布前应当在二者中选一个（都需要一次 ADR）：

1. 把数据目录迁到 `%LOCALAPPDATA%\local.devchievements.companion`（identifier 制，
   也是 Tauri 惯例），顺带做 §40.1 的「数据迁移」；
2. 或改成 `perMachine` 装进 `Program Files`，代价是需要管理员权限。

### 1.6 两种「签名」不是一回事（§40.3 / §40.4）

| | 状态 | 作用 |
| --- | --- | --- |
| 更新签名（minisign，`.sig`） | **已做**，每次打包都产出 | 证明这个包来自持有密钥的人，updater 安装前校验 |
| 代码签名（Authenticode） | **未做**，没有证书 | 决定 Windows 是否显示 SmartScreen 警告、发布者名字是否可信 |

所以准确说法是：安装包**有更新签名，没有代码签名**。
前者挡不住 SmartScreen，后者也挡不住伪造更新——两者不能互相替代。
§40.4 的状态因此保持 `Blocked`，需要的证书记在
`docs/development/external-accounts-and-secrets.md`。

## 2. 更新机制（§40.3）

§40.3 列了五件事：签名与完整性、失败回滚、数据库与配置迁移、中断恢复、失败不得破坏数据库。
前两件交给 Tauri Updater；后三件它**不可能**替我们做，因为它不知道我们有一个 SQLite 数据库。

### 2.1 为什么用插件而不是自己写下载器

自己写下载器意味着自己实现签名校验。签名校验写错的典型形态是「校验失败时放行」，
而那正是最危险的一种 bug：用户以为装的是我们的包。

插件的公钥写在 `tauri.conf.json` 的 `plugins.updater.pubkey`，
私钥在仓库之外（`_toolchain/updater/`，见 `.gitignore` 之外的独立目录，不入库）。

一条测试守住这个配对关系：`update::the_endpoint_list_matches_the_config`
用 `include_str!("../tauri.conf.json")` 读真实配置，比对公钥非空、endpoints 数量与
`update::ENDPOINTS` 一致。配置和代码分头改动时会直接测试失败，而不是上线后才发现。

### 2.2 `configured: false` 比一个假 URL 诚实

现在没有发布feed，所以 `plugins.updater.endpoints` 是空数组，
`--update-status` 输出：

```json
{ "configured": false, "endpoints": 0, "currentVersion": "0.1.0", "snapshotPresent": false }
```

`configured` 为 false 就是「没有可问的地方」，而不是「问过了没有更新」。
填一个不存在的 GitHub 仓库地址可以让界面好看，但那是编造。
`scripts/e2e/installer.mjs` 在**安装后的**程序上断言 `configured === false`，
所以这条诚实性是被测出来的，不是写在注释里的。

### 2.3 数据库安全网

| 函数 | 语义 |
| --- | --- |
| `snapshot_database` | 版本变更前把 `companion.db` 复制成 `companion.db.pre-update`；没有数据库时返回 `None` |
| `has_snapshot` | 有没有待确认的副本 |
| `restore_snapshot` | 把副本放回去（先删 live，Windows 上覆盖被占用文件会直接失败） |
| `clear_snapshot` | 新版本启动并迁移成功后丢弃副本 |
| `open_companion_database` | 启动路径：打开+迁移成功就清副本；失败则还原并重试一次 |

语义是「**新版本证明自己之后**才丢弃退路」，而不是打包时丢。
这样一次失败的迁移会把「用户丢了历史」变成「这次更新没发生」。
6 个单元测试覆盖：无数据库不产生空副本、副本是复制不是移动、还原后 live 内容回退、
成功后副本被删且 live 不被回退、没有副本时还原必须报错（不能静默成功）。

### 2.4 还没做的

- **没有发布 feed**：`endpoints` 为空是产品事实，不是遗漏。发布时填
  `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`，
  同时改 `update::ENDPOINTS`，漂移测试会强制两边一致。
- **界面没接**：`update_status` / `prepare_update` / `clear_update_snapshot` 三个命令已注册，
  但设置页还没有「检查更新」按钮。前端需要 `@tauri-apps/plugin-updater` 包，
  当前环境无法安装 npm 新依赖（离线），因此按 `Partially Implemented` 记录。
- **真实下载安装未验证**：没有 feed 就无法端到端跑通下载→验签→安装→重启，
  这条只能是 `Platform Unverified`。

---

## 3. 发布流水线（§40.2）

`.github/workflows/release.yml`，两个 job：

| job | 内容 |
| --- | --- |
| `verify` | fmt / clippy / test / build / wasm / **desktop app test** / typecheck / extension build |
| `windows` | 构建 UI → 构建 helper → `tauri-action` 打包签名 → 检查 `.sig` 存在 |

### 3.1 只构建 Windows

§40.2 写的是「Windows、macOS、Linux 构建；按实际支持范围构建 x64 与 ARM64」。
支持范围是 Windows（ADR-0017），所以 macOS/Linux 的 job 不会产出任何有人用的东西，
只会把「CI 绿了」变成一种与产品无关的信号。这一点在文件头写明，不是悄悄省略。

### 3.2 为什么用 `tauri-action`

它做三件手工脚本容易漏的事：调用 bundler、把 `TAURI_SIGNING_PRIVATE_KEY` 交给签名、
`includeUpdaterJson` 生成并上传 `latest.json`——那正是 §40.3 需要的更新 feed。

### 3.3 少一个 `.sig` 就失败

最后一个步骤检查 `bundle/**/*.sig` 是否存在。没有签名的安装包**无法被更新**，
而「发布后发现更新链路是断的」是不可逆的（用户已经装了那个版本）。所以让它在发布前失败。

### 3.4 工作流自身的检查

环境里没有任何 YAML 解析器（查过 `node_modules`、`.pnpm`、bundled Python 的 PyYAML，都没有），
为两个文件引入一个解析器不划算。于是写了 `scripts/check-workflows.mjs`——
**它明确不是 YAML 解析器**，只查真正会让工作流坏掉的四件事：制表符、非 2 倍缩进、
同一映射内重复键、`uses:` 没锁版本。

它自己被验证过：故意写一个带重复 `run:`、制表符和未锁版本 `uses:` 的文件，报出 4 个问题后退出 1。
一个永远通过的检查器等于没有检查器。

## 4. 性能实测（§44.1 / §44.4）

§44.1 的四项目标里，有两项从 Phase 0 起就写着「未测量」，理由是「无 Companion」。
现在有可用的 Companion 了，这个理由过期了。§44.2 又明确禁止编造数字，
所以这一段只有实测值。

### 4.1 方法（先说方法，再说数字）

| 脚本 | 量什么 | 端点 |
| --- | --- | --- |
| `scripts/perf/hook-latency.mjs` | Hook 耗时 | 从外层脚本 spawn hook 到进程退出；另有对照 `cmd /c exit` 量「起一个进程本身要多久」，和一次完整 `git commit` |
| `scripts/perf/commit-to-notification.mjs` | Commit → 通知 | 从 `git commit` 之前到**提示音开始播放**（§44.4 允许「首帧渲染**或**提示音」） |
| `scripts/perf/idle-cpu.mjs` | 空闲 CPU | 固定时间窗内**整棵进程树**的 `TotalProcessorTime` 增量 |

三个选择需要解释：

1. **端点用提示音，不是每次都等弹窗。** 大部分提交只解锁声音（§16.4），
   等弹窗的样本会变成「等一件不会发生的事」。声音是 §44.4 明文接受的端点，
   而且每一条提交都会产生，200 个样本就是 200 次真实通知。
   弹窗首帧另外收集，样本数单独报，不混进 p95——两者回答的不是同一个问题。
2. **CPU 量整棵进程树。** 窗口是 WebView2，有自己的子进程；
   只量我们自己启动的 exe，会得出一个用户在任务管理器里看不到的数字。
3. **时间戳由当事进程自己打。** 靠观测者轮询日志会把轮询延迟算进延迟里，
   所以 `POPUP shown/painted`、`SOUND play` 都带 `at=<unix ms>`。

**测量环境**（§44.4 要求记录）：Windows 11 Pro 10.0.26300，
Intel Core Ultra 7 255HX（20 逻辑核），git 2.54.0.windows.1，
debug 构建，200 样本，原始数据在 `scripts/perf/.out/*.json`。

### 4.2 第一次测：超预算 2 倍

```text
samples: 200
sound start  min=214  p50=737  p95=1004  max=1872 ms
target: p95 < 500 ms -> OVER
```

原因不在 Engine，也不在 SQLite：drain 循环的休眠是 `50 × 10 ms`，
也就是**每 500 ms 才看一次 spool**。预算本身就是 500 ms，
所以光是等待就吃光了全部预算；Windows 还会把 10 ms 的 sleep 向上取整到计时器节拍，
于是实际 p95 到了 1004 ms。

### 4.3 改成 5 × 10 ms 之后

同一个夹具连跑两次（每次 200 样本、0 缺失）：

| 运行 | min | p50 | p95 | max |
| --- | --: | --: | --: | --: |
| A | 212 ms | 260 ms | **274 ms** | 641 ms |
| B | 214 ms | 262 ms | **328 ms** | 598 ms |

弹窗首帧只有 1 个样本（387 / 424 ms），只作记录。

### 4.3.1 中间有个 p95=489 的假结果，值得留下

改完之后第一次重测得到 p95 **489 ms**，差点就要在「刚好达标」的边上写结论。
复查脚本才发现问题在测量工具自己：它每 2 ms 把整份 app 日志读一遍并重新 split，
到第 200 个样本时日志已经几十 KB，相当于测量进程在跟自己测的对象抢 CPU。
改成按字节偏移增量读取之后，同样的构建连跑两次得到 274 和 328。

教训：**性能数字的第一次出现不等于真相，先怀疑测量工具。**

### 4.4 代价：空闲 CPU 涨了 11 倍

| | 45 s 窗口 CPU 时间 | 占单核 | 占整机 |
| --- | --- | --- | --- |
| 500 ms 轮询 | 93.8 ms | 0.208% | 0.01% |
| 50 ms 轮询 | 1078.2 ms | 2.396% | 0.12% |

（两行是同一个脚本、同一个夹具、修轮询前后各跑一次；`scripts/perf/.out/idle-cpu.json`
只保留后一次的报告。要复现前一行，把 `main.rs` 里 drain 循环的 `0..5` 改回 `0..50`。）

这是**用空闲 CPU 换延迟**，必须说清楚：按整机口径 0.12% 仍远低于 1%，
但按单核口径 2.4% 已经不便宜了——每 50 ms 醒一次、扫一遍空目录，本身就是浪费。
下一步应该是给 spool 目录挂文件系统通知（有事件立刻醒、没事件不醒），
把「快速反应」和「空闲安静」同时拿到；在那之前，这一条按**整机口径**达标，
并且**只在「只有托盘、没有开窗口」的状态下测过**。开窗口时的 WebView2 子进程未测。

### 4.5 Hook 耗时（出货的 helper，不是 spike）

```text
control (cmd /c exit)  min=12.63  p50=13.30  p95=14.59 ms
hook                   min=8.63  p50=9.35  p95=10.81  max=52.66 ms  (n=200)
full git commit        min=156.49  p50=175.88  p95=196.02 ms
```

两个值得注意的地方：

- hook 的 p95 **比「启动一个 cmd.exe」还低**（10.81 vs 13.30）。
  这说明这个数字主要不是「Windows 起进程很慢」，helper 自己确实很快；
  没有对照的话，10.81 ms 到底算快算慢只能靠感觉。
- 一次完整 `git commit` 的 p50 是 175.88 ms，其中 helper 只占约 9 ms。
  换句话说：如果用户觉得提交慢，怪 Hook 是错的。

### 4.6 这些数字只在以下前提下成立

- **debug 构建**（`cargo build` 无优化）：所以是上界，release 只会更快。
- 200 个样本是**连续提交**，属于「忙」的情形；真实用户隔几分钟提交一次，
  第一条提交最多多等一个轮询周期（≈50 ms），仍在预算内。
- 弹窗首帧只有 **1 个样本**（387–424 ms），不足以谈 p95，只作为参考记录。
- 两次独立运行的 p95 是 274 与 328，说明这套测量仍有约 50 ms 的口径抖动；
  报「p95 ≈ 300 ms」比报某一次的具体数字更诚实。
- 机器：见 `scripts/perf/.out/*.json` 里的 `device` / `os` / `cpus` / `git`。

### 4.7 顺带发现的真问题

为了造 200 次通知，最初的夹具装了 200 个阈值递增的自定义成就（`at_least: 11..210`，
正好是 `MAX_USER_ACHIEVEMENTS`）。结果一条都没解锁。查下去发现：
**`userAchievements` 只被写入，从未被 Engine 读取**——
`crates/achievements` 提供定义与校验，`crates/core` 只评估内置成就，中间没有任何一环把它们接起来。

也就是说：用户在界面里做出来的成就，保存成功、导出正常、然后永远不触发。
这已经不是性能问题，是 §22 的功能缺口，已改记为「已知问题（产品层面）」并触发 STATUS 的 4a.8 重新定级。
性能夹具因此改成「用提示音端点」，不再依赖自定义成就。

## 5. 启动行为：是否隐藏到托盘（用户要求）

原来的启动逻辑只有一条：首次运行开窗口，之后一律只留托盘。这是「companion 应该安静」
的默认值，但它把用户的选择权拿走了——有人每天早上都要看仪表盘，每次都得去托盘点一下。

### 5.1 三条开窗理由，按优先级

| 条件 | 行为 | 为什么在这个位置 |
| --- | --- | --- |
| `--open-window` | 开窗 | 脚本与试用启动器需要确定性，优先级最高 |
| 首次运行（向导未完成） | 开窗 | 身份和 Hook 没配好就静默运行，等于什么都没做 |
| 设置里关掉「启动时隐藏」 | 开窗 | 用户的显式选择 |
| 其余情况 | 只留托盘 | 这才是默认的 companion 行为 |

### 5.2 存储与兼容

新字段 `startHidden`（默认 `true`）：

- `Settings`（`settings.json`）与 `ConfigDocument`（导入导出格式）都带上了它，
  导出再导入不会丢；
- 两边都用 `#[serde(default)]` 兜底，所以**这次改动之前导出的配置文件仍然能导入**，
  缺字段时取「隐藏到托盘」；
- 一条测试专门守这件事：`an_export_written_before_start_hidden_still_imports`。

### 5.3 怎么验证的（不靠肉眼看屏幕）

启动时打印一行 `STARTUP hidden=<bool> setting_start_hidden=<bool>`，于是两种取值都能断言：

```text
--- startHidden=True ---
STARTUP hidden=true setting_start_hidden=true
--- startHidden=False ---
WINDOW opened reason=start-visible
STARTUP hidden=false setting_start_hidden=false
```

「窗口到底开没开」这件事以前只能看屏幕，现在是一条可断言的日志。

## 6. 开机自启动（用户要求）

### 6.1 为什么用 Run 键而不是启动文件夹快捷方式

卸载器（tauri 的 NSIS 模板）已经会删 `HKCU\...\CurrentVersion\Run\${PRODUCTNAME}`。
所以写这个值，是**唯一一种卸载时会顺带被清掉**的自启动方式；
放一个 shell:startup 快捷方式的话，卸载后它会留在那里，指向一个不存在的程序——
那是典型的「装得干净、卸不干净」。

| 项 | 值 |
| --- | --- |
| 位置 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| 名称 | `Devchievements`（与卸载器删的名字一致） |
| 数据 | `"<exe 绝对路径>"`，只带引号，不带参数——登录时启动应当和双击完全一样 |

默认 **关**。装一个软件不等于允许它在你登录之前就运行；打开这个开关才算同意。
导入配置文件时也默认关（`#[serde(default)]`），避免一次导入悄悄加上自启动。

### 6.2 设置是唯一真相，注册表跟着它走

启动时调用 `autostart::reconcile(setting)`：

| 设置 | 注册表现状 | 动作 |
| --- | --- | --- |
| 开 | 值指向本 exe | 重写（程序搬家后路径要对） |
| 开 | 没有值 / 指向别处 | 写入 |
| 关 | 值指向本 exe | 删除 |
| 关 | 没有值 / **指向别的安装** | 不动 |

最后一行是有意的：机器上可能同时存在另一个 Devchievements（不同目录的旧版本），
关掉本实例的自启动不该把别人的条目删掉。判定写在 `points_at()` 里，
路径比较忽略大小写（Windows 语义），有 4 个单元测试守着，包括「指向别的副本时返回 false」。

### 6.3 改开关时立刻生效

`save_settings` 不只写文件，还当场 `autostart::apply(...)`——
一个要等下次启动才生效的开关，用户会当成坏掉的开关。

### 6.4 验证到了哪一步（诚实版）

| 部分 | 状态 |
| --- | --- |
| 判定逻辑（`points_at` / `command_for`） | `Tested`，4 个单元测试 |
| `--autostart status/on/off` 命令行 | `Tested`：`status` 输出 `{"enabled":false,"setting":false}` |
| **真正的注册表写入** | **本环境无法验证**：沙箱拒绝子进程写 HKCU，返回 `WIN32_ERROR(5)`（拒绝访问）；`pwsh` 自己写同一个键是成功的 |

所以 `apply()` 在真实机器上的行为记为 `Platform Unverified`，并在错误里带上 Win32 状态码——
下次看到的是 `WIN32_ERROR(5)` 而不是「打不开 Run 键」。要现场确认，在**正常 cmd** 里：

```bat
devchievements-desktop.exe --autostart on
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v Devchievements
devchievements-desktop.exe --autostart off
```

## 7. 向导可达性：一个「测试走的不是产品路径」的经典 bug

### 7.1 症状

干净数据目录下首次启动，窗口直接打开**空仪表盘**，向导不出现。用户永远看不到身份确认和
Hook 安装这两步，于是既不会统计到任何东西，也找不到原因。

### 7.2 根因

`main.rs` 里有**两个** `tauri::Builder`：正常启动一个，`--window-smoke`（供 e2e 逐页渲染）一个。
两边各自维护一份 `generate_handler!` 命令清单，而它们**漂移了**：

| 命令 | 正常启动 | `--window-smoke` |
| --- | --- | --- |
| `onboarding_status` | ❌ 缺 | ✅ |
| `install_hooks` | ❌ 缺 | ✅ |
| `import_history` / 资源导入 / 清空数据 / 预览弹窗 | ❌ 缺 | ✅ |

UI 拿不到 `onboarding_status` 时走的是 `.then(ok, () => setOnboarded(true))` 的失败分支——
「读不到状态就当已完成，至少别把用户拦在门外」。于是**命令缺失被解释成「已经配置过了」**，
仪表盘打开了，向导消失了。

而 `scripts/e2e/wizard.mjs` 全部用 `--window-smoke` 启动，那个清单是完整的，
所以 14 项检查一路绿灯。**测试走的是产品不走的那条路**——这正是最该被抓住的一类假通过。

### 7.3 修法：让漂移不可能发生

命令清单收进一个宏 `app_commands!()`，两个 builder 都调它：

```rust
macro_rules! app_commands {
    () => {
        tauri::generate_handler![ /* 唯一一份清单 */ ]
    };
}
```

三份复制变一份，之后再想漏一个命令，得同时改掉两边（因为它们已经是同一份）。

### 7.4 修法之二：让测试走产品路径

`wizard.mjs` 新增 `realLaunch()`：用**真实启动参数**（`--open-window`，不带 `--window-smoke`）
起一个实例，等日志里出现 `UI READY view=welcome`，再杀掉。
原来的 smoke 检查保留（它负责逐页渲染），但「首启是否出现向导」现在由真实路径断言——
这条用例在修复前会失败。

### 7.5 顺带补上的「重新运行向导」

一个只能走一次的配置向导，一旦走岔就没有第二条路（仓库 hook 就是在这一步装不上的）。所以：

- 命令 `reset_onboarding()`：只清 `onboardingCompleted`，**不动**历史、成就和其它设置；
- 设置页 **Data → Setup → Re-run setup**：调用后 `window.location.reload()`，外壳重新读标志、
  渲染向导。用 reload 而不是路由，是因为向导由外壳渲染，页面自己切不过去；
- CLI `--reset-onboarding`：同一个函数，供支持与测试调用。

### 7.6 验证

`scripts/e2e/wizard.mjs` 现在 18 项，全部通过：其中 4 项是新加的——真实启动出现向导、
重置成功、重置后再次启动出现向导、重置后身份配置仍在（证明只清标志不删数据）。

## 8. 多语言（§40.1）

### 8.1 设计：英文原文就是 key

没有另造一套 key（`settings.startHidden.label` 这类），而是让**英文句子本身当 key**：

```ts
t("Start hidden in the tray")   // 中文时返回「启动时隐藏到托盘」，其它语言时原样返回
```

两个后果都是想要的：

1. **漏译退化成英文，而不是露出 key。** 半翻译的界面难看，但不会变成 `i18n.missing.key`
   这种谁也不知道是什么意思的东西；
2. 英文不需要字典，原文只有一处，改文案时不会出现「改了英文忘了改 key」。

占位符用 `{name}`，由 `tf()` 填充（例如 `{n} 次提交`）。复数规则暂时不引入：
需要它的句子都写成不需要的形态，而中文本身没有复数。

### 8.2 语言怎么解析

| 设置值 | 行为 |
| --- | --- |
| `en` | 英文 |
| `zh-CN` | 简体中文 |
| `system`（默认） | 读 webview 的 `navigator.language`，以 `zh` 开头就是中文 |

`system` 在**前端**解析而不是 Rust：答案在 webview 的 locale 里，
那才是这台机器上其它程序显示的语言。设置文件里的值只负责「用户想怎样」。

未知值（手改或新版本写入）由 Rust 的 `normalize_language()` 收敛成 `system`，
不会让窗口变成没有文案。

### 8.3 怎么证明它真的生效

只断言「设置里写着 zh-CN」是自欺欺人——界面完全可能还在显示英文。
所以窗口渲染后会把**解析出的语言和一个已翻译的词**一起报给 Rust：

```text
UI LANGUAGE lang=zh-CN sample=仪表盘
UI LANGUAGE lang=en sample=Dashboard
```

`pages` e2e 断言这两行。`sample` 非空且来自字典，才说明查表真的发生了。

### 8.4 漏译检查

`scripts/check-i18n.mjs`：从源码里抽出所有 `t("...")` / `tf("...")` 的 key，
和 `i18n.ts` 里的字典比对，缺一条就退出 1；顺带报告字典里没人用的条目。

当前：**178 条 key，178 条译文，0 缺失**。

（这个过程本身踩过一个坑：检查器最初把源码里的 `\\` 当成两个字符比较，
于是带 Windows 路径的文案永远「缺译」；改成两边都先解码成运行时字符串才对齐。）

### 8.5 覆盖范围

九个界面文件全部走 `t()`：外壳与导航、仪表盘、成就馆、活动、仓库、关于、设置、
**首次配置向导**、**弹窗**。

弹窗原来把「成就已解锁」写死成中文，英文用户会看到中英混排——这次一并修掉。

## 9. 启动前置检查与扩展引导（用户要求）

向导现在是 **七步**：

```text
welcome → checks → identity → hooks → import → extension → done
```

新增的是两端：进门先查依赖，末尾引导扩展。

### 9.1 为什么要自己查一遍（安装包不是已经处理了吗）

| 依赖 | 安装包 | 应用自己查的理由 |
| --- | --- | --- |
| WebView2 | NSIS 会跑微软的 bootstrapper | 它是**静默**的：下载被拒或安装失败的机器上，程序能启动但画不出任何东西 |
| Git | **完全不检查**（Tauri 不认识 Git） | 没有 Git 就没有 hook，产品没有任何东西可看 |

所以 `preflight::check()` 读两个真实来源：

- **WebView2**：注册表三处（含 `WOW6432Node`）的 `Clients\{F3017226-...}\pv`。
  只认真实安装写的键，**不认文件存在**——那个值我们自己也能造假；
- **Git**：先查 `PATH`（hook 将来用的是这个），再查 `Program Files\Git\cmd\git.exe` 等常规安装位置，
  因为「在 Git 装好之前就开着的 shell」PATH 是旧的，只查 PATH 会给出**假阴性**。

程序**不会替你装任何东西**。它只报告缺什么、去哪装，装不装是用户的决定——
替用户装软件不在我们该做的事里（AGENTS.md 7.8）。

### 9.2 启动时开控制台的问题，也在这里收口

`git --version` 如果走 `Command::new` 默认会闪一个控制台窗口。这是同一个坑的第三次：
所以这个模块里的进程调用一律带 `CREATE_NO_WINDOW`。

同样的处理也用在了新的 `open_external`（用 `explorer.exe` 把链接交给默认浏览器），
并只放行 `http(s)`：URL 虽然来自我们自己的报告，但桥那一头的东西不该能借这个口子启动 `file:` 或自定义协议。

### 9.3 扩展引导：说真话

扩展还没上架，所以这一页不能写「点击安装」。它写的是：

1. 扩展是**可选**的——本地 Git 提交已经由主程序统计；
2. 扩展补的是只有浏览器看得见的东西（PR、发布、Issue）；
3. 手动安装路径：`edge://extensions` 或 `chrome://extensions` → 开发人员模式 → 加载解压缩的扩展；
4. 数据仍然只在本机。

顺带修掉向导第三步一句**不实的承诺**：原本写「之后可以在 Repositories 页添加更多仓库」，
而那个页面只有导入、没有安装 hook 的入口。现在改成「之后要再加仓库，在设置里重新运行本向导」——
这是目前唯一真实存在的路径。

### 9.4 验证到哪一步

| 部分 | 状态 |
| --- | --- |
| 检查逻辑与报告形状 | `Tested`，2 个单元测试 |
| 预检页渲染 + 两个依赖都被检查 | `Tested`，`scripts/e2e/dependencies.mjs` 5 项 |
| 向导七个步骤都能渲染 | `Tested`，`wizard.mjs` 逐页渲染 |
| **缺少依赖时给出的建议** | **无法在本机验证**：这台机器两样都有，测试不能卸载 Git 去证明。需要在一台干净虚拟机上确认 |

## 10. 没有 WebView2 时的原生兜底（用户指出）

用户指出一个真实的死角：**如果机器上没有 WebView2，程序会直接报错退出**——
而我们上一轮做的「依赖预检」面板，本身就是要用 WebView2 画出来的。
用画不出来的界面去说明「你缺东西所以画不出来」，是自相矛盾。

### 10.1 分层：哪种缺失可以在界面里说，哪种不能

| 缺失项 | 能否在界面里说明 | 处理 |
| --- | --- | --- |
| Git | 能——没有 Git 只是没有数据，窗口照画 | 向导第 2 步 `checks` 里报告 + 给下载入口 |
| WebView2 | **不能**——没有它就没有窗口 | 启动时用**原生消息框**说明，然后 `exit(2)` |

所以判断放在建窗口**之前**，且在 `main()` 的最外层：

```text
没有 WebView2 → MessageBoxW（中英双语 + 下载地址 + 「现在打开吗？」）
                → 点「是」用默认浏览器打开微软的下载页
                → exit(2)
```

### 10.2 几个刻意的选择

- **双语写死**：消息框出现在读得到语言设置之前，没法用字典，所以两种语言都写。
- **`exit(2)`**：把「缺运行库」和「程序崩溃」区分开，不是同一个故障类。
- **CLI 模式豁免**：`--diagnostics`、`--onboarding` 这类命令不建窗口，也就不需要 WebView2。
  支持场景下它们是唯一的取证手段，不能被预检拦住。
- **`--webview2-prompt`**：可以强制弹出这个框，便于人工确认文案（也便于我截图/演示）。

### 10.3 安装包那一层本来也在做

`bundle.windows.webviewInstallMode` 之前是**默认值**，现在显式写成：

```json
{ "type": "downloadBootstrapper", "silent": true }
```

显式而不是默认：这条决定「安装包会不会替你装 WebView2」，不该藏在默认值里。
它是静默的，所以失败时用户看不到任何提示——这正是应用自己再查一遍的理由。

### 10.4 验证到哪一步

| 部分 | 状态 |
| --- | --- |
| 判定（`must_stop_for_missing_webview`） | `Tested`，2 个单元测试 |
| 文案（双语 + URL） | `Tested`，断言含 `WebView2`、下载地址、`无法启动` |
| CLI 不受影响 | `Tested`：`--onboarding` 照常输出 JSON，正常启动照常开窗 |
| **消息框真的弹出来** | 人工可验：`devchievements-desktop.exe --webview2-prompt`。模态框无法自动化断言 |
| **真·没有 WebView2 的机器** | **未验证**：本机有运行库，不能为了测试卸载它。需要一台干净虚拟机 |

### 10.5 这层兜底第一版是坏的（用户复现）

第一版把「有没有命令行参数」当作「是不是命令模式」：

```rust
// 错的：--open-window 也是 -- 开头，被当成命令，于是守卫被跳过
let cli_mode = std::env::args().skip(1).any(|a| a.starts_with("--"));
```

而试用启动器传的正是 `--open-window`（一个 GUI 标志）。结果：机器缺运行库时，
守卫被跳过，程序照样去建窗口，然后直接报错——**正是用户看到的现象**。

改成一个显式的命令清单 `COMMAND_FLAGS`，GUI 标志不在里面：

| 标志 | 归类 |
| --- | --- |
| `--open-window` / `--page` / `--step` / `--webview2-prompt` | GUI |
| `--diagnostics` / `--onboarding` / `--autostart` / `--window-smoke` … | 命令 |

同一个判断现在被两处共用（日志重定向、WebView2 守卫），不会再各写一份。

### 10.6 让这条路径**可验证**

缺运行库的机器不好找，所以加了两件东西：

1. 启动时永远打一行 `WEBVIEW2 version=…`——下次「明明有却还是报错」时，这行是分辨依据；
2. `DEVCH_FORCE_NO_WEBVIEW2=1` 强制把运行库当作缺失。它最坏只能让程序弹个框然后退出，
   不构成对用户的攻击面。

`dependencies` e2e 因此能断言两件**用户实际踩到的事**：

```
ok   a missing runtime is reported before any window
ok   no window was created first
```

第一条用 `--open-window` 启动，第二条证明日志里没有 `WINDOW created/opened`——
也就是说守卫确实在造窗口之前拦住了。这条用例在上一版代码上会失败。

### 10.7 虚拟机实测：注册表在说谎（用户复现）

用户在精简版 Windows 11 虚拟机里实测，看到的是：

1. 一个**空白窗口**（WebView2 画不出来）；
2. 紧接着 wry 自己弹的英文错误框：“Could not find the WebView2 Runtime.”

而我们的双语提示框没有出现。原因很具体：**那台机器的注册表里 EdgeUpdate 客户端键还在，
`pv` 还有版本号，但 `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\<pv>\` 里没有
`msedgewebview2.exe`**。第一版只读注册表，于是判定「已安装」，直接去建窗口。

这是「用间接证据代替事实」的典型错误：注册表是**安装记录**，不是**运行库本体**。

修法：注册表给出的版本**必须在磁盘上真的有 `msedgewebview2.exe`** 才算数，
另外三个位置都查（`ProgramFiles(x86)`、`ProgramFiles`、`%LOCALAPPDATA%`，覆盖机器级与用户级安装）。
注册表为空但磁盘上找得到运行时的情况，也算存在。

```text
WEBVIEW2 registry says 157.0.4341.46 but the runtime files are missing
WEBVIEW2 missing: prompting for installation
```

第一条是新增的诊断行：下次「注册表说有、实际没有」时会直接写进日志。

三个单元测试守住这条边界（用临时目录造出三种布局）：

| 场景 | 期望 |
| --- | --- |
| 注册表版本对应目录不存在 | 视为缺失 |
| 注册表为空但存在任一版本目录 + exe | 视为存在 |
| 版本目录在但 exe 缺失 | 视为缺失 |

## 11. 诊断导出：脱敏默认、原始需明示（§26.2）

§26.2 要求「诊断包导出默认脱敏；用户可以选择包含原始信息，但必须有明确警告」。
之前只有前者，这一轮补齐后者。

### 11.1 两种形态，不同用途

| 形态 | 内容 | 用途 |
| --- | --- | --- |
| 脱敏（默认） | 仓库名→稳定别名；数据路径中的用户名打码；文件内 `"redacted": true` | 贴到公开的 issue |
| 原始（需明示） | 真实仓库标识与路径；文件内 `"warning"` 说明它不适合公开分享 | 私下发给维护者 |

关键实现细节：**警告写在文件里**，不只是界面上。文件会被转发，而转发时那句说明不会跟着走；
打开文件的人必须能自己看出这是什么。

### 11.2 三个入口

```text
--diagnostics                       打印脱敏报告（stdout，纯 JSON）
--export-diagnostics <path> [--raw] 写到文件
设置页 → 诊断 → 导出诊断（带「包含真实仓库名和路径」勾选 + 红色警告）
```

命令与 CLI 共用 `diagnostics_report(data_dir, reveal_raw)`，所以两条路径不可能给出不同结论。

### 11.3 一处被测试抓住的回归

为了让「机器有没有 WebView2」可诊断，我在启动时打了一行 `WEBVIEW2 version=…`，
结果 `--diagnostics` 的 stdout 不再以 `{` 开头——`diagnostics` 与 `config-io` 两个 e2e
立刻红了（`SyntaxError: Unexpected token 'W'`）。

这不是测试脆弱，是**约定的输出被污染**：`--diagnostics` 的输出是给程序读的。
于是改成：

- banner 只在 **GUI 启动**时打（命令行模式不打）；
- 版本号写进诊断报告本身（`"webview2": "157.0.4341.46"` 或 `null`），机器可读、更有用。

### 11.4 验证

`scripts/e2e/diagnostics-export.mjs`（9 项）断言：默认脱敏、脱敏文件无警告字段、
数据路径里不含用户名、原始导出带 `raw=true`、原始文件自带警告且说明「不适合公开分享」。

另外 `diagnostics`、`config-io`、`pages`、`dependencies` 四个既有 e2e 与 `verify.mjs` 全部通过。

## 12. 依赖检查搬进安装过程（用户要求）

「缺 WebView2 无法启动」这个问题，**在启动后才处理已经太晚**。用户的要求是对的：
检查应该在安装阶段，而不是装完再告诉用户跑不起来。

### 12.1 安装包自己的检查为什么会漏

tauri 的 NSIS 模板这样判断：

```nsis
ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\...\EdgeUpdate\Clients\{F3017226-...}" "pv"
${If} $4 == "" ... 才去下载/安装 WebView2
```

**注册表有版本号就整段跳过。** 精简版 Windows 上正是「键在、`msedgewebview2.exe` 不在」，
于是安装包认为已装、照常装完，应用再画一个白窗口然后死在 wry 里。

### 12.2 改成：安装前查文件，查不到就装，装不上就不装

`apps/desktop/src-tauri/installer-hooks.nsh`，挂在 `NSIS_HOOK_PREINSTALL` 上——
这个位置在 `SetOutPath $INSTDIR` 之后、**第一个 `File` 之前**，所以失败时磁盘上什么都没写，
**不需要回滚**。

流程：

```text
1. 读三处注册表的 pv
2. 拿这个版本去三个位置找 msedgewebview2.exe（ProgramFiles64 / ProgramFiles / LocalAppData）
3. 找不到 → 运行随安装包内嵌的官方 bootstrapper（/silent /install）
4. 再查一次，仍然没有 → 提示 + 打开下载页 + 退出码 2，一个文件都不装
5. Git 只警告不拦截（没有 Git 程序照样能跑，只是没有提交可记）
```

`webviewInstallMode` 因此改成 `{"type":"skip"}`：WebView2 只有**一处**逻辑（我们的），
不会出现「模板装了、我们又装一次」或者「模板跳过了、我们不知道」。

### 12.3 两个刻意的细节

- **`Quit` 而不是 `Abort`**：静默安装下 Abort 会让进程悬着不退出（实测），`Quit` + `SetErrorLevel 2`
  既立刻退出又给出可判断的退出码。
- **`${IfNot} ${Silent}` 才弹框**：静默安装（CI、`/S`）不能卡在消息框上。

### 12.4 验证

| 场景 | 结果 |
| --- | --- |
| 本机（有 WebView2）静默安装 | 退出码 0，主程序 / helper / sounds 全部就位 |
| 强制缺运行库（`DEVCH_FORCE_NO_WEBVIEW2=1`） | **退出码 2，`devchievements-desktop.exe` 未安装** |
| 安装后卸载 | 退出码 0，程序删除、数据保留 |
| `scripts/e2e/installer.mjs` | 19 项全过（新增「缺运行库时安装失败且什么都没装」两项） |

内嵌的 `MicrosoftEdgeWebView2Setup.exe` 来自微软官方下载，**Authenticode 签名校验通过**
（`CN=Microsoft Corporation`），1.9 MB，安装包从 4.96 MB 变成 6.45 MB。

### 12.5 改为「直接装」，不是「告诉用户去装」（用户要求）

第一版只是**检查**：缺了就用 bootstrapper 装 WebView2，Git 缺了只给个提示。
用户要求改成**安装过程中直接把缺的组件装上**。现在两条都装：

| 组件 | 安装方式 | 失败后 |
| --- | --- | --- |
| WebView2 | 内嵌官方 bootstrapper，`/silent /install` | 退出码 2，一个文件都不装 |
| Git | 先 `winget install --id Git.Git -e --silent`，失败再下载**固定版本**官方安装器，`/VERYSILENT /CURRENTUSER` | 警告后继续装（没有 Git 程序仍能运行，只是没有提交可记） |

三个决定值得记下来：

1. **Git 用 `winget` 优先**：它自己处理签名、代理和用户级路径，是微软维护的更新通道；
   没有 App Installer 的机器才回退到独立安装器。
2. **URL 固定版本，不跟 `latest`**：`git-scm.com/download/win` 是网页不是文件，
   GitHub 的 `latest` 重定向也可能变成 HTML。写死 `v2.56.1.windows.1` 的资产地址，
   每次构建拿到的都是**同一个二进制**。
3. **`/CURRENTUSER`**：我们的安装器是 `currentUser` 模式、不请求管理员权限，
   Git 安装器默认往 Program Files 装会失败。

静默安装下这条链会真的去下载 63 MB 的 Git——只在缺的时候。测试用
`DEVCH_FORCE_NO_GIT=1` / `DEVCH_FORCE_NO_WEBVIEW2=1` 分别强制两条分支，
不必把 Git 从构建机上卸掉。

实测：

| 场景 | 退出码 | 结果 |
| --- | --- | --- |
| 依赖齐全 | 0 | 安装成功 |
| 强制缺 Git | 0 | 安装成功 + 提示（Git 不是门禁） |
| 强制缺 WebView2 | 2 | 未安装任何文件 |

## 13. MSIX 实测：商店路线对「执行打包内的 exe」是硬阻塞

为了回答「MSIX 上商店是否可行」，我打了一个真实 MSIX 并装在本机测（不是推测）。

### 13.1 做了什么

`scripts/msix/build-msix.mjs`：

- 手写 `AppxManifest.xml`（`Windows.FullTrustApplication` + `runFullTrust`，并声明 `zh-cn`）；
- 载荷与 NSIS 包一致：主程序、`devch-helper.exe`、`sounds/`；
- `makeappx pack` → `signtool` 用自签证书签 → 提权信任证书并 `Add-AppxPackage`；
- `scripts/msix/probe-packaged-app.mjs` 逐项验证。

### 13.2 结果

| 检查 | 结果 |
| --- | --- |
| 包能打出来、能签名、能安装 | ✅ 装到 `C:\Program Files\WindowsApps\...` |
| 打包目录只读（数据必须搬出安装目录） | ✅ 确认 |
| 打包后的程序能被启动 | ✅ 进程起来了，托盘存在 |
| 程序能往用户仓库写 Git hook | ✅ `.git/hooks/post-commit` 写入成功 |
| **hook 能否执行包内的 `devch-helper.exe`** | ❌ **拒绝访问** |

最后一条是决定性的：

```text
BLOCKED MethodInvocationException: 对“Start”的调用引发异常：拒绝访问
```

`Program Files\WindowsApps` 下的可执行文件**只能由包身份启动**，普通进程（包括 git 拉起的 hook）
执行它会被拒绝。而本产品的核心机制恰恰就是「git → hook → helper」。

这不是权限没配好：ACL 里 `BUILTIN\Users` 有 `RX`，但 WindowsApps 的执行限制在 ACL 之上，
属于包模型本身。

### 13.3 结论

**MSIX 路线在当前架构下不可行**，除非改动产品核心：

| 可能的绕法 | 是否可接受 |
| --- | --- |
| hook 直接调用主程序而不是 helper | 仍是被打包的程序，同样被拒 |
| hook 里内联采集逻辑（不调 exe） | 等于放弃 Rust helper，重写采集，且无法复用已测代码 |
| 把 helper 复制到包外再调用 | 包内程序可以复制自身到 `%LOCALAPPDATA%` 再执行——**这是唯一可能的路，但等于把 helper 装到包外**，商店审核是否接受需要再确认 |

所以**商店（MSIX）与「Git hook 型产品」存在结构性冲突**。若将来真要上商店，唯一有希望的方向是第三条，
且需要先确认审核口径。

这条结论现在有实测支撑，不再是猜测；相关脚本保留在 `scripts/msix/`，将来立场变化时可以直接重跑。

## 14. 商店路线（MSIX）：从「不可行」到「跑得通」

§13 的结论是 MSIX 不可行，卡在 hook 无法执行包内的 helper。用户决定走商店路线后，
找到了绕法并**实测通过**。

### 14.1 绕法：把 helper 复制出包

```text
包内（WindowsApps）                数据目录（可写）
devch-helper.exe  ──复制──▶  %LOCALAPPDATA%\devchievements\bin\devch-helper.exe
                                      ▲
                                      └── hook 指向这里
```

依据是同一次测量：**同一个二进制**在包内执行被拒绝，复制到用户目录后 `exit 0` 并正常投递记录。

实现要点（`src-tauri/src/local_helper.rs`）：

- 路径固定为 `<data>/bin/devch-helper.exe`，**不随版本变化**，所以升级不会让已装的 hook 失效；
- 每次启动比对「长度 + 修改时间」，不同才覆盖；
- **先写 `.tmp` 再改名**：半截的可执行文件被 commit 撞上，比没有 helper 更糟；
- `install_hooks` 与启动都走同一个函数，两条入口不可能给出不同路径。

### 14.2 打包侧

| 项 | 做法 |
| --- | --- |
| 真图标 | 从 `icon.ico` 生成 5 个 Store 尺寸（Pillow），**不再是纯色占位** |
| 开机自启 | manifest 里 `windows.startupTask`（属于 `<Application>` 的扩展，不是包级），默认 `Enabled="false"` |
| 自启逻辑 | 打包运行时 `autostart::reconcile` **不写 Run 键**，改为记 `AUTOSTART packaged=true managed_by=manifest` |
| WebView2 缺失 | bootstrapper 随包分发（`webview2-setup.exe`），原生框选「是」→ **先复制出包再运行** → 重新探测 → 成功则继续启动，不必重启 |
| 数据目录 | 包外（`%LOCALAPPDATA%\devchievements`），卸载不删 |

### 14.3 实测（`scripts/msix/probe-packaged-app.mjs`，10 项全过）

```text
ok  the package is installed
ok  the payload is complete
ok  the package folder is read-only
ok  the packaged app can install a Git hook
ok  the app copied the helper out of the package
ok  the hook calls the copy, not the packaged file
ok  a commit still succeeds with the packaged helper
ok  the hook delivered a record
ok  the helper runs from the managed copy
ok  the data directory is outside the package
```

### 14.4 仍未验证 / 已知缺口

| 项 | 状态 |
| --- | --- |
| 商店审核是否接受「写入用户 Git 仓库 + 复制可执行文件到用户目录」 | **未验证**，属于审核判断 |
| 提交时的认证备注 | 已写好：[store-certification-notes.md](store-certification-notes.md)，逐条对照代码核实过 |
| 图标质量 | 源 `icon.ico` 只有 32×32，放大到 310 会糊；**需要真正的美术资源** |
| 无 WebView2 的机器上「一键安装」全流程 | 本机有运行库，**只能在你的虚拟机验** |
| `fixedRuntime`（把 130MB 运行时打进包） | 未做；当前用 bootstrapper 按需安装，包体保持 6.5MB |
| 打包后的 GUI 启动日志 | 通过 `shell:AppsFolder` 启动时没有写入 `companion.log`，进程确实在跑；原因未查（CLI 模式正常） |


## 15. 首屏三个 `undefined`：线格式与 TypeScript 类型不一致

做商店截图时发现的，而且**已经存在于发出去的构建里**：

| 位置 | 现象 | 原因 |
| --- | --- | --- |
| 最长连续 | `undefined 天` | `Statistics` 序列化成 `longest_streak`，界面读 `longestStreak` |
| 存储 | `schema vundefined` | `StorageHealth` 同样是 snake_case |
| 最早的事件 | `Invalid Date` | `oldest_event_at` 读不到，`new Date(undefined)` |
| About 页 | **整页白屏** | `Capabilities` 也是 snake_case，`channel(undefined)` 里 `.replace` 抛异常 |

共同原因：这些结构体直接跨 IPC 边界，但只有外层 DTO 加了 `rename_all = "camelCase"`，
内部的领域模型没加。**没有任何测试比对过 JSON 键名和另一侧的期望**，所以一路绿到最后。

修法：给 `ActivityDay`、`Statistics`、`StorageHealth`、`RepositoryEntry`、`Capabilities`
加 `rename_all = "camelCase"`，并补 5 个线格式测试（断言 JSON 的键集合，而不是字段值）。

### 15.1 加了什么守卫

1. **线格式测试**：Rust 侧断言导出的键名集合，与 TS 接口逐字段对齐；键一改，测试红。
2. **页面自检**：界面在 2.5 秒后扫一次 `document.body.innerText`，发现
   `undefined` / `Invalid Date` / `[object Object]` 就通过 `ui_error` 写进 `companion.log`，
   并打印 `UI PLACEHOLDER none on page <id>` 表示干净。
3. **`scripts/e2e/ui-pages.mjs`**：逐页打开真实窗口，断言「打开的是请求的那一页」且日志里
   有 `UI PLACEHOLDER none`。这条同时验证了 `--open-window --page` 参数真的生效
   （之前被静默忽略，所以每次截图都是仪表盘）。

第 2 条立刻就抓到第 4 行那个白屏：本轮第一次跑就报
`UI ERROR TypeError: Cannot read properties of undefined (reading 'replace')`。


## 16. 预览按钮死锁：同步命令 + 等待 = 主线程自己等自己

用户报告「预览弹窗完全没反应」，之前我两次都**绕开了按钮**（用 `--preview-popup` 从 Rust 调用），
所以一直没复现。这次用 webview 的调试协议**真的去点那个按钮**，一次就复现：

```text
before:  POPUP icon_url=none / POPUP render radius=0 ...   ← 之后永远没有下文
after:   POPUP placed at=1917,1407 ... POPUP preview shown ... POPUP painted
```

原因是一条 Tauri 规则：**同步命令跑在主线程**。

```rust
app.run_on_main_thread(|| popup::show(...));  // 排到主线程
receiver.recv_timeout(10s)                     // 也是主线程 → 死锁
```

而这个等待**是当天早上我自己加的**（为了让失败能显示出来而不是假报已打开）。想要结果没错，错在线程。
改成 `pub async fn`（命令移到工作线程）就好了。

### 16.1 同一症状下叠着三个缺陷

| # | 缺陷 | 引入时间 | 现象 |
| --- | --- | --- | --- |
| 1 | 定位用整个显示器而非工作区 | 一直存在 | 弹窗被任务栏压住 |
| 2 | 用弹窗窗口判断显示器 | 一直存在 | 多屏时弹到别的屏 |
| 3 | 同步命令 + 等待 = 死锁 | **当天引入** | 完全无反应 |

第 3 条是「改完要用真实点击验证」的直接教训：
用 `--preview-popup` 验证，等于测了一条用户点不到的路。

### 16.2 新增的两个真实点击测试

| 脚本 | 断言 |
| --- | 
## 17. 浏览器扩展连接：协议通了，但发现一个计数缺陷

用户要求「检查应用能否正确连接浏览器扩展」。逐层验证的结果：

| 检查项 | 结果 |
| --- | --- |
| Native Messaging 注册表项 | ✅ Chrome 与 Edge 都有 `com.devchievements.host` |
| host manifest | ✅ 存在，`allowed_origins` 指向扩展 id `gfgbmedanlpminnlbbdefojbgphoekbj` |
| host 二进制 | ✅ 存在（指向 `target\debug`，**这是构建输出，不是安装位置**）|
| 协议本身 | ✅ 手工按浏览器格式（4 字节长度 + JSON）发送，`stored: true` |
| Spool 写入 | ✅ `pending-*.spool` 出现，内容正确 |
| Companion 消费 | ✅ `DRAIN read=1 ingested=1` |
| **扩展是否已安装** | ❌ **未安装**（Edge/Chrome 的用户配置里都没有这个 id）|

### 17.1 发现并修掉的缺陷：PR 被算成提交

浏览器报告一个合并的 PR 后，数据库出现：

```text
event:   PullRequestMerged
commit projection: 1        ← 错了
unlocked: first_commit      ← 错得更明显
```

原因：`spool_consumer` 把**每一条**记录都送进 `ingest_commit_with_rules`，
而它唯一的事实输入是 `CommitFacts`——PR 于是按「一次提交」计数并触发了提交类成就。

修法：只有 `EventType::CommitCreated` 才构造真实提交事实，其余事件用
`counts_as_commit: false`，事件照样入库，统计不动。实测修复后：

```text
events: PullRequestMerged
commit projection: []        ← 不再计数
unlocked: []                 ← 不再误解锁
```

回归测试 `engine::tests::facts_that_do_not_count_produce_no_changes` 守住 Engine 一侧。

### 17.2 仍未完成的部分

| 项 | 状态 |
| --- | --- |
| PR / Release / Issue 的统计与成就 | 当时**没有**（`pull_request_merged_count` 在 achievements crate 里有字段，但 Engine 只评估提交）。已在 §18 补上 |
| 已安装包内的 native host | **没有**。MSIX 里不含 host 二进制，注册表指向构建目录；正式分发需要把它打进安装包并在安装时注册 |
| 扩展在真实浏览器里连通 | 当时**未验证**（扩展没有安装，端到端只测到 host 进程这一层）。已在 §18.1 于真实 Edge 中验证 |

---
## 18. 浏览器活动真正产生成就与弹窗（§13.2 / §16.4）

§17 修掉的是「PR 被算成提交」。修完之后出现的是**相反方向的缺陷**：事件入库了，
但什么都不会发生——没有计数、没有成就、没有弹窗。用户看到的是「连上了，但没反应」，
而这两件事在浏览器那一端完全无法区分。

### 18.1 先让真实链路通

按顺序实测（每一步都看真实文件/数据库，不看日志里的期望值）：

| 步骤 | 实测 |
| --- | --- |
| 扩展在 Edge 里加载 | 未打包加载，id `gfgbmedanlpminnlbbdefojbgphoekbj`（`Secure Preferences` 里 `disable_reasons: []`）|
| PR 页面产生报告 | `[devchievements] event pull_request_opened pr:1 delivered` |
| host → Spool | `spool\pending-*.spool` 出现，内容为 `browser_github` + `pr:1` |
| Companion 消费 | **首次仍 `REFUSED`**（见 18.2）|

### 18.2 「保存了映射却还是不认」

拒绝原因是 `author is not a configured email`：浏览器报告只带 GitHub 登录名，
身份要靠 `githubLogins` 映射。映射保存进 `settings.json` 后**依旧被拒**。

原因：drain 循环只在启动时读一次 `settings.json`。修法是**每一轮重读**
（身份 + 自定义成就规则），理由写在代码里：一个保存了却要重启才生效的映射，
用户会认为它已经生效。

实测同一份 Spool 记录（`event_id: browser:github:applepinea/dsh-qq-access#pull_request_opened:pr:1`）：

```text
修复前  REFUSED 1 record(s): author is not a configured email
修复后  IDENTITY from settings.json
        INGESTED 1
```

数据库里现在有这一行（`PullRequestOpened`，`BrowserConfirmed`，`local_date 2026-11-05`）。

### 18.3 补上浏览器活动的评估路径

| 层 | 改动 |
| --- | --- |
| Engine | 新增 `BrowserActivity` 与 `evaluate_browser_event`：只移动自己的计数列、解锁 §13.2 的浏览器成就、**不**产生 `CommitRecorded`、**不**移动每日等级（§14.3 只数提交）|
| 读集 | `ReadSet::BROWSER`：读提交总数（浏览器事件也可能满足提交成就）、读已解锁集合、读用户汇总；**不读**日投影 |
| 存储 | `repository_projection` 加 5 列（PR Merge / Release / Issue Opened / Repository Created / Star Given），成就读 `SUM`；schema 1 → 2，旧库用 `PRAGMA table_info` + `ALTER TABLE` 幂等升级 |
| 入库分发 | 按 `EventType` 分发，不再让所有记录走提交路径；用户自定义规则用**同一份**统计快照判定 |
| 重建 | `rebuild_from_events` 一起重算这 5 个计数，并为「只有浏览器活动的仓库」建行（提交数为 0 是事实，不是缺行）|

ADR：[0019](../decisions/0019-browser-activity-counters.md)。

### 18.4 验证

- Rust：4 个测试锁住语义——只动自己的计数、不产生提交确认、不移动每日等级、
  第二次合并不重复庆祝；存储侧 4 个测试覆盖计数/解锁/outbox、只入库不计数、
  零提交仓库的行、重建等于实时累加。
- `scripts/e2e/browser-achievement.mjs`（22 项，真二进制 + 真数据库）：
  合并的 PR → `NOTIFY popup title="Merged"` + `first_pr_merged` + 计数 1 且
  提交计数仍为 0；打开的 PR → `ingested=1` 且**无**弹窗；重复报告 → `dup=1` 且
  不重复庆祝；Release → `First Release`；三天前的合并 PR → 解锁但**不弹窗**
  （这条断言最初是失败的，见 §19）。

### 18.5 仍未验证 / 未做

| 项 | 说明 |
| --- | --- |
| 真实「合并 PR」的浏览器事件 | 需要一次真实 merge；目前同一条路径用合成的记录验证过（与 host 写出的格式逐字节一致）|
| 浏览器提交数 vs 提交成就的跨仓库口径 | 提交总数仍按**仓库**统计（既有行为）。浏览器事件用的是本仓库提交数 + 用户汇总的浏览器计数；「用户所有仓库的提交总数」这个口径没有改，单独记为待办 |
| 已安装包内的 native host | MSIX 仍不含 host 二进制，注册表指向 `%LOCALAPPDATA%\devchievements\bin` |
## 19. 一个被「计数」掩盖的缺陷：过期事件其实照样弹窗（§16.3）

补浏览器成就时写了这条断言：三天前的合并 PR 应当**只更新统计、不弹窗**。
它失败了——弹窗照常出现，而同一份日志里 `DRAIN ... queued=0`。

### 19.1 原因

新鲜度策略（§16.3）在消费端只做了一件事：把「排队数」改成 0。

```rust
notifications: if matches!(fresh, Silent) { 0 } else { queued.len() }
```

但 Outbox 行**已经写进数据库**了，而 outbox 的状态机只认「行是否存在」——
于是这一行照常被投递、照常弹窗。**报表说没排队，屏幕上有弹窗**，
两者都不算错，错的是它们描述的不是同一样东西。

### 19.2 修法

把新鲜度变成**写入端的策略**，而不是投递端的过滤器：

| 策略 | 记录解锁 | 写 Outbox | 解锁来源 |
| --- | --- | --- | --- |
| `Inform` | ✅ | ✅ | realtime |
| `Quiet`（新增） | ✅ | ❌ | **realtime**（事件刚发生在用户账号上，只是我们知道得晚）|
| `Silent`（历史导入） | ✅ | ❌ | import |

理由写在代码里：**存在即投递**。任何「先写再过滤」的实现都会再犯同一个错，
所以安静必须体现在「不写」。

### 19.3 实测

`browser-achievement` e2e 新增一段：三天前的合并 PR → `ingested=1`、
**无** `NOTIFY popup`、`first_pr_merged` 仍然入库且 `unlock_source=realtime`。
存储层另有一个单测固定「Quiet 不写 Outbox 行」。

### 19.4 一个仍然存在的近似

10 分钟–24 小时这一档，§16.3 要求「汇总通知」。汇总形态没有实现，
现在仍然是一条完整弹窗。这是**已知的未做**，不是「已符合规范」：
在半成品汇总和用户应得的庆祝之间，选择了后者，并记在这里。
## 20. 「没有任何弹窗」：一次可以修好的拒绝被当成了删除

用户的反馈是四个字：**并没有弹窗**。查现场后，原因不是弹窗坏了，而是报告压根没进到
判定——它被身份闸门拒绝了，然后**被删掉了**。

### 20.1 现场证据

`companion.log`：

```text
IDENTITY from settings.json
REFUSED 1 record(s): author is not a configured email   (×6)
```

Spool 里 6 条被消费的记录，内容都是：

```json
{"activity":"pull_request","author_login":"Applepinea","merged":false,
 "event_id":"browser:github:applepinea/awesome-dsh-plugin#pull_request_opened:pr:1", ...}
```

而 `settings.json` 里的映射只有 `{"qq985065985": ["csah1350@outlook.com"]}`。

两个独立的原因叠在一起：

| # | 原因 | 说明 |
| --- | --- | --- |
| 1 | 账号没关联 | 页面上的作者是 `Applepinea`，映射里只有 `qq985065985`，所以 §7 拒绝 |
| 2 | 打开的 PR 本来就没有成就 | `merged: false` → `PullRequestOpened`，规则集里没有任何一条读它（§16.4 无此行）|

即使账号关联正确，一个**只是打开**的 PR 也不会弹窗。这是规范行为，但它和「坏了」
在用户那里长得一模一样——而这正是要修的地方。

### 20.2 最严重的一条：拒绝等于删除

`Spool` 记录在 drain 结束时被移到 `consumed/`，无论结果是被接受还是被拒绝。
于是「账号还没关联」——一个点一下就能修好的设置问题——变成了**永久丢失的活动**，
而 UI 上什么也没有。日志里那行 `REFUSED` 是唯一的痕迹。

### 20.3 修法

| 层 | 改动 |
| --- | --- |
| Schema v3 | 新表 `browser_reports_rejected`：保存**原始 payload**，所以拒绝是可回放的，不是账本上的一个计数 |
| 消费端 | 浏览器报告被抓到拒绝时入库停放；本地提交不参与（那是仓库 `user.email` 的问题，重放只会得到同样的拒绝）|
| 设置页 | 新增「等你处理」区块：列出未关联账号和待回放条数，一个「关联到我的邮箱」按钮 |
| 回放 | 关联时把停放的 payload 写回 Spool 并立即 nudge drain，走**同一条**消费路径 |

为什么走 Spool 回放而不是直接调 ingest：回放必须和扩展投递走完全一样的路径，
否则「回放能用」和「真实投递能用」就是两套代码，迟早只对一半。

### 20.4 验证

`scripts/e2e/unlinked-login.mjs`（15 项，真二进制 + 真数据库）：

```text
未关联报告 → refused=1，无弹窗
           → browser_reports_rejected 里有 1 行（login=Applepinea）
--link-login Applepinea → LINKED replayed=1，映射写入 settings.json，停放表清空
再跑一次 → ingested=1，refused=0，弹窗「Merged」，first_pr_merged 解锁，
           pull_request_merged_count=1 且 commit_count=0
```

存储层另有单测（重复停放不重复计数、payload 逐字保留、清理生效）。

### 20.5 仍然存在的两件事

| 项 | 状态 |
| --- | --- |
| 用户已有的 6 条被删记录 | **找不回来**（它们在本功能之前就被消费删掉了）。重新打开一次 PR 页面即会重新上报 |
| 打开的 PR 没有任何可见反馈 | 仍然如此。它现在会显示在「等你处理」（如果账号没关联），关联后则只是入库——规范没有为它定义通知，这一条没有擅自加 |
