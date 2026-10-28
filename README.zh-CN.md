<h1 align="center">Agent HUD</h1>

<p align="center">
  <strong>为你的 agents 打造的 HUD。</strong><br>
  在屏幕上，在手机上，在手腕上。
</p>

<p align="center">
  Agent HUD 的跨平台版，由原生 Swift 版 <a href="https://github.com/jazzenchen/agent-hud-native-open">agent-hud-native-open</a> 迁移而来。
</p>

<p align="center">
  <a href="https://agenthud.app/mac"><img src="docs/buttons/mac.zh-CN.svg" alt="下载 Mac 版 Agent HUD" width="180" height="36"></a>
  <a href="https://apps.apple.com/cn/app/agent-hud/id6812113619?uo=4"><img src="docs/buttons/app-store.svg" alt="从 App Store 获取 iPhone 与 Apple Watch App" width="138" height="36"></a>
  <a href="#从源码构建"><img src="docs/buttons/source.zh-CN.svg" alt="从源码构建 Agent HUD" width="181" height="36"></a>
</p>

<p align="center">
  <a href="#从源码构建"><img src="docs/badges/macos.svg" alt="macOS 12+" height="22"></a>
  <a href="#跨平台一套核心"><img src="docs/badges/windows-linux.svg" alt="Windows 与 Linux 预览" height="22"></a>
  <a href="docs/platform-architecture.md"><img src="docs/badges/rust-tauri.svg" alt="Rust 与 Tauri 2" height="22"></a>
  <a href="LICENSE"><img src="docs/badges/license.svg" alt="Apache-2.0 许可证" height="22"></a>
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>简体中文</strong> · <a href="https://agenthud.app">官网</a> · <a href="#支持的-agent">支持的 agent</a> · <a href="#文档">文档</a>
</p>

<p align="center">
  <img src="docs/videos/agent-hud-loop.zh-CN.webp" alt="灵动岛上的 Agent HUD 演示。光晕呼吸着红、黄、绿的额度色段；悬停后面板展开，显示 Claude 与 Codex 的额度、DeepSeek 的余额、过去 24 小时的 Token 与会话，点击后 Claude 的额度切换为消耗速率。随后 Claude 请求运行测试，用「批准一次」回应；它的回复以「有新回复」提醒出现，卡片上有「回到会话」" width="640">
</p>

Agent 在 Mac 上工作，HUD 把进展带到你身边。看一眼状态，接上一段对话，给出下一个决定——在电脑前，也在离开电脑之后。

## 在屏幕上

**你的 agents，一目了然。** 你在写代码、读文档或处理别的事情，agents 继续工作。屏幕边缘告诉你：谁还在忙，谁已经完成，谁正在等你。

细节就在手边，需要你批准的事情也在这里回应。多个 agent 的进展，集中在一眼可见的地方。

<p align="center">
  <img src="docs/videos/simple-mode-tour.zh-CN.webp" alt="简约形态停靠在屏幕左侧的一段演示。五个 agent 图标外是额度圆环，随后显示百分比与余额。悬停在图标上，用一行写出该 agent 的额度；点击后展开 Claude 的卡片，显示额度、消耗速率与 24 小时的 Token 柱。接着用 Allow once 回应一个批准请求，展开一条 New reply 提醒及其 Return to session 按钮，最后把 HUD 拖到屏幕顶部再拖回" width="640"><br>
  <sub>停靠在屏幕左侧的简约形态，已开启「按 agent 分开」 · 示例数据</sub>
</p>

- **光晕**：每个 agent 一种颜色，按它用量最高的那项额度：已用不足 70% 为绿色，达到 70% 为黄色，达到 90% 为红色；额度耗尽前会提醒你。有 agent 在工作时，光晕动得更快。
- **悬停**：额度及其重置时间或消耗速率、余额、过去 24 小时的 Token，以及会话，等你处理的排在最前。
- **请求**：批准、拒绝、总是允许，回答 agent 的提问，都在 HUD 上完成，Claude 桌面版里的提问也一样。Agent HUD 从不替你做决定：你没处理的请求，10 分钟后交还 agent。
- **跳转**：一键回到会话所在的线程、标签页或终端窗格。

**三种形态。** 每块屏幕各用一种：有刘海的屏幕默认是刘海，其余默认是动态。

| 形&#8288;态 | 屏幕上的样子 |
| --- | --- |
| **刘&#8288;海** | 在刘海处，周围是光晕。 |
| **简&#8288;约** | 停靠在任意屏幕边缘的小型悬浮面板：每个 agent 的图标外套一个圆环，画的是它用量最高的那项额度，旁边写着百分比。 |
| **动&#8288;态** | 沿任意屏幕边缘排成一排的 agent 图标，背后是光晕。agent 工作时，它的图标会变成 Bot。 |

## 在手机上

**需要做决定时，不必停下脚步。** 离开电脑，工作仍然接得上。在 iPhone 上查看进展、阅读回复、继续对话。

agent 需要你做决定时，在手机上就能看清上下文，回答问题或批准下一步。任务继续，你的生活也继续。已支持 iPhone Duo。

<p align="center">
  <a href="docs/screenshots/iphone-home.webp"><img src="docs/screenshots/iphone-home.webp" alt="iPhone 版 Agent HUD 的首页：Claude 与 Codex 的额度卡片，列出每个窗口的已用比例与重置时间，以及 DeepSeek 的账户余额" width="150"></a> <a href="docs/screenshots/iphone-sessions.webp"><img src="docs/screenshots/iphone-sessions.webp" alt="iPhone 版 Agent HUD 的会话页：今天各类 Token 的用量与费用，下方是带每轮柱状图与上下文占用的会话" width="150"></a> <a href="docs/screenshots/iphone-duo-stats.webp"><img src="docs/screenshots/iphone-duo-stats.webp" alt="展开的 iPhone Duo 上的 Agent HUD 统计页：折痕左侧是按模型堆叠的 Token，右侧是今天、近 7 天与近 30 天的用量" width="455"></a><br>
  <sub>iPhone 版 Agent HUD · 首页与会话，以及展开的 iPhone Duo 上的统计页 · 示例数据</sub>
</p>

详见 [Agent HUD Remote](#agent-hud-remote)。

## 在手腕上

**从手腕开始，在 iPhone 上继续。** 散步时想到一个需求，抬腕说出来，直接发送给家中的 Mac。一个随手捕捉的想法，就能成为正在推进的任务。

在手表上查看最新回复；想展开讨论、补充细节时，在 iPhone 上继续。想法从手腕开始，对话随着你展开。

<p align="center">
  <img src="docs/screenshots/watch-record.zh-CN.webp" alt="Apple Watch 正在录一条新笔记：波形、0:07、中文与停止按钮" width="180"> <img src="docs/screenshots/watch-note.zh-CN.webp" alt="Apple Watch 上的笔记文字，已在 iPhone 上转写，下方是「发给 agent」按钮" width="180"> <img src="docs/screenshots/watch-send.zh-CN.webp" alt="发给：Claude 与 Codex，均为桌面版" width="180"> <img src="docs/screenshots/watch-replied.zh-CN.webp" alt="同一条笔记标着「Codex 已回复」，下方是「查看最新消息」按钮" width="180"><br>
  <sub>对 Apple Watch 说出的一条笔记，成为 Mac 上的一个 Codex 会话 · 示例数据</sub>
</p>

详见 [Agent HUD Remote](#agent-hud-remote)。

## 内置 MCP

**你的 agents 也在用 Agent HUD。** 内置的 MCP 服务给它们一条互相通话的频道，也让它们按你的想法搭出界面。

### 让 agents 彼此对话

Claude Code 写完一段改动，可以请一个 Codex 会话审一遍，Codex 的答复回到 Claude 手里；也可以新开一个会话，把任务交给它。Agent HUD 本来就看得见每个会话，所以能在不同客户端之间传话。

谁在跟谁说话，HUD 上看得见：发信的 agent 把信折成纸飞机，飞到收信的 agent 那里，回答再沿原路飞回来。

- **目前打通了 Claude Code 与 Codex。**
- **不打断工作**：消息等对方空闲才送达，不会插进正在进行的一轮。
- **在[发行版](https://agenthud.app/mac)中提供。**

### 打造你自己的界面

告诉 agent 你想怎样看信息：“把 Codex 放在前面，只保留我关心的数据。”

通过 MCP，调整 Mac HUD 的排列、信息和外观。即时预览，满意后保存，让看板贴合你的工作习惯。

- 适用于 Claude Code、Codex 和其他 MCP 客户端。
- Agent 可以读取 HUD，为任意一块屏幕提出布局或外观，并用截图检查效果；截图中的账户会被遮住。
- 同一组工具也能回答关于用量、会话与统计的问题，数据来自应用已有的记录。

## 为什么值得常驻

### 看清 Token 用量

知道 agents 做了什么，也知道它们用了多少。查看 Token 消耗、回看用量变化，了解消耗分布在哪些 agent、模型和会话。

额度告诉你还剩多少，Token 用量帮助你看懂资源如何被使用。

| 维度 | 可以追踪 |
| --- | --- |
| **时间** | 过去 5 小时、24 小时、7 天或 30 天，柱宽从 15 分钟到 1 天。历史在你的电脑上保留 30 天。 |
| **Token&nbsp;类别** | 输入、输出、推理、缓存写入与缓存读取，可以全看，也可以任意组合。 |
| **Agent&nbsp;与模型** | 每个 agent 的占比、Token 与费用，以及用到的模型。 |
| **会话** | Token（含子 agent 的部分）、费用、轮数、调用数、上下文占用与缓存命中。子 agent 计入启动它的会话。 |
| **轮次与调用** | 每一轮新增的 Token，以及每次调用的模型、上下文和它请求的工具。 |
| **缓存命中** | 今天、近 7 天、近 30 天，每个会话，以及会话里的每个模型。 |
| **费用** | 按各家的 API 标价估算（中国区或国际区价目），标 ≈，并非实际扣费。 |
| **活跃时段** | 过去 7 天里，agents 在哪些钟点工作。 |
| **余额** | DeepSeek 每天的花费，以及余额预计用完的日期。 |

### 让 agents 活起来

熟悉的 agent 标志变成小 Bot，用动作表达思考、工作和等待。状态有了表情，你扫一眼就更容易理解正在发生什么。

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/videos/agent-bots-row-dark.webp">
    <img src="docs/videos/agent-bots-row-light.webp" alt="Agent HUD 支持的各个 agent 的标志排成一行。一道波浪从左到右掠过：每个标志醒来变成 Bot 开始工作，有几个带着感叹号跳起来请求批准，完成时露出笑脸，再变回标志" width="840">
  </picture>
</p>

它们也让 HUD 有了自己的性格：你的 agents 在这里忙碌，需要你时向你示意。每个 Bot 沿用图标自己的形状和颜色，眼睛跟随指针；Claude Code 的 Bot 是 Clawd。[查看每个 Bot 的四种状态](docs/videos/agent-bots.webp)。

### 占用小，不拖慢你的 Mac

HUD 需要长时间运行，轻量常驻本身就是产品体验。控制 CPU、内存和能耗开销，把系统资源留给 agents 和你手头的工作。

这一点用实际资源占用来证明：

<!-- 数字来自按 docs/performance.md 测量的发布构建。 -->
| 指标 | 闲时 | 忙时 | 构成 |
| --- | --- | --- | --- |
| CPU（单核占比） | 约&nbsp;7% | 7%–&#8288;28% | Agent HUD 本身 6%–11%，每块屏幕的 HUD 0.5%–7%，图形与网络进程最多 5% |
| 内存 | 约&nbsp;240&nbsp;MB | 240–&#8288;260&nbsp;MB | Agent HUD 自身进程，即活动监视器里 Agent HUD 一行的数字。每块屏幕的 HUD 另占 70–170 MB，图形进程 10–50 MB，网络进程 7 MB |
| 8&nbsp;小时的能耗 | 约&nbsp;0.1&nbsp;Wh | 0.1–&#8288;2.3&nbsp;Wh | 一半以上来自 Agent HUD 本身，其余是 HUD 与图形进程 |

闲时：没有 agent 在工作，一块屏幕。忙时：agents 在写日志，一到两块屏幕。发布构建，Apple M1 Max，macOS 26，本机约 8,000 个会话日志。[测量方法](docs/performance.md#measured-footprint)

## 跨平台，一套核心

Agent HUD 基于 Tauri 2 构建：Rust 核心、React 界面，每个操作系统各有一层很薄的原生代码。

- **一套核心。** 读取 agent 的记录、统计 Token、计算额度、价格与会话，都只在 Rust 里做一次，每个系统的结果完全一致。窗口只负责显示核心算好的内容。
- **每个系统各有自己的文件。** 路径、进程、凭据、回调与窗口，每个系统各写一份；改进 Windows 或 Linux，不会碰到共享代码。
- **macOS 已可用，Windows 与 Linux 处于预览。** 两者都能从本仓库源码构建，但尚未在真机上验证；还缺的部分见[即将推出](#即将推出)。
- **iPhone 与 Apple Watch 与 Mac 配对。** Agent HUD Remote 经由 iCloud 同步，因此只在 macOS 上提供。

## Agent HUD Remote

**走到哪里，agents 都在身边。** 经由你自己的 iCloud，iPhone 与 Apple Watch 在任何网络下都连着[发行版](https://agenthud.app/mac)。

### 你将获得

iPhone 和发行版在同一个网络里，不订阅也能在 iPhone 与 Apple Watch 上看到 Mac 上的额度、Token 统计与会话。Remote 再加上：

- **任何网络**：额度、Token 统计与会话，出门在外也看得到。
- **对话**：阅读 Claude Code、Codex、Antigravity 与 DeepSeek Harness 的会话，并向 Claude Code 与 Codex 发送消息。
- **决定**：允许或拒绝一次工具调用，回答提问，批准计划。
- **手腕上**：说出的想法成为 Mac 上的新会话，回复回到你的手表。
- **提醒与实时活动**：iPhone 与 Apple Watch 上的提醒，以及锁定屏幕和灵动岛上运行中的会话。

| | iPhone | Apple Watch |
| --- | :---: | :---: |
| 额度、余额与 Token 统计 | ✓ | ✓ |
| 会话及其 Token | ✓ | ✓ |
| 主屏幕、锁定屏幕与待机显示的小组件 | ✓ | — |
| 多台 Mac | ✓ | ✓ |
| 对话 | ★ | ★ |
| 给 Claude Code 与 Codex 发消息 | ★ | ★ |
| 一条笔记变成新会话 | ★ | ★ |
| 工具调用、提问与计划 | ★ | ★ |
| 提醒 | ★ | ★ |
| 实时活动里的运行中会话 | ★ | ★ |

<sub>✓ 免费使用 · ★ 需要订阅</sub>

**在 App Store 订阅**，在 iPhone App 里完成：[中国区](https://apps.apple.com/cn/app/agent-hud/id6812113619) · [国际区](https://apps.apple.com/us/app/agent-hud/id6812113619)

## 即将推出

- **Windows 与 Linux 结束预览**：Windows 上的客户端回调，以及能安装回调的 Linux 安装包。[欢迎参与](CONTRIBUTING.md)。
- **支持更多 agent**，不止下面表格里的 18 款。
- **按 Token 计费的 API**：用 API 密钥付费的账户，也能看到用量、花费与余额，和订阅的额度放在一起。
- **UI 提示词模板。** 现成的提示词，一次就把 HUD 改成你想要的样子。
- **子 agent 甘特图。** 在会话页上，一轮里的每个子 agent 各占一条泳道，排在同一条时间轴上：何时开始、跑了多久、用了多少，哪些在同时进行。
- **所有界面交给 agent 定制**，不只是 HUD；支持图片背景，iPhone 上的界面也能自定义。
- **更多 agent 加入对话**，不止 Claude Code、Codex、Antigravity 与 DeepSeek Harness。
- **文件双向传送。** agent 可以把文件发到你的 iPhone，你也可以从 iPhone 把文件发给 agent。
- **录音直接发给 agent。** Apple Watch 或 iPhone 上的录音，原样交给 agent。
- **HUD 上更多由你决定**：简约形态的圆环显示什么（消耗速率、距重置的时间或费用），以及哪些 agent 可以提醒你。
- **每个 agent 更完整**：Grok CLI 的额度与一键回到会话，OpenClaw 的实时状态与回复，Devin 的实时状态。
- **从源码构建的版本也能在 HUD 上处理请求与回复**，在客户端回调结束预览之后。
- **鸿蒙版。** 鸿蒙手机的 App 正在提交审核：与[发行版](https://agenthud.app/mac)在同一局域网内时，查看额度、Token 统计与会话，桌面上还有服务卡片。

……以及更多。

## 支持的 agent

安装并登录想要关注的 agent，Agent HUD 会自动发现它们。

| Agent | Token 与会话 | 额度 | 实时状态 | 批准 | 提问 | 回复 | 回到会话 | 轮次与调用 |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **Claude&nbsp;Code** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Codex** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **DeepSeek&nbsp;Harness** | ✓ | 余额 | ✓ | ✓ | ✓ | ✓ |  | ✓ |
| **Antigravity** | ✓ | ✓ |  | ✓ | ✓ | ✓ | ✓ |  |
| **Cursor** | ✓ | ✓ |  |  |  | ✓ |  |  |
| **Grok&nbsp;CLI** | ✓ |  | ✓ |  |  | ✓ |  |  |
| **Grok&nbsp;Bot** | ✓ | ✓ |  |  |  |  | ✓ |  |
| **GitHub&nbsp;Copilot&nbsp;CLI** | ✓ | ✓ | ✓ |  |  | ✓ |  |  |
| **OpenCode** | ✓ | ✓ |  |  |  | ✓ | ✓ |  |
| **Kimi** | ✓ | ✓ | ✓ |  |  | ✓ |  |  |
| **Pi** | ✓ | ✓ | ✓ |  |  | ✓ | ✓ |  |
| **OpenClaw** | ✓ |  |  |  |  |  |  |  |
| **Hermes&nbsp;Agent** | ✓ |  |  |  |  |  |  |  |
| **ZCode** | ✓ |  |  | ✓ | ✓ |  |  |  |
| **CodeBuddy** | ✓ |  |  | ✓ |  | ✓ |  |  |
| **WorkBuddy** | ✓ |  |  | ✓ |  |  |  |  |
| **Qwen&nbsp;Code** | ✓ |  |  | ✓ |  | ✓ |  |  |
| **Devin&nbsp;CLI** | ✓ | ✓ |  | ✓ |  | ✓ |  |  |

另外还读取 **GLM Coding Plan** 的额度，并支持批准 **Qoder** 系列的请求。Codex 的批准需先在 Codex 中信任回调，DeepSeek Harness 的批准和提问需先安装 Agent HUD 插件（设置 → 智能体插件），Copilot 的额度需先在设置中同意读取，Devin 的额度来自其 CLI 的本机缓存。Cursor 的 Token 需要 Cursor 3.27 或更新版本。Grok Bot 的 Bot 运行在 Cursor 的服务器上，由服务器给出每个 Bot 每天的 Token；已合并到 SuperGrok 的账户没有这项数据，只显示会话。

## 保护你的数据

- **在你的电脑上读取**：各 agent 已经保存的记录。用量历史保存在本机的 SQLite 数据库中。
- **向各厂商读取**：额度、余额，以及 Grok Bot 每天的 Token，使用 agent 现有的登录状态或已配置的密钥。任何一次读取都不会向模型发送消息，凭据不会被保存。
- **在你的电脑上写入**：agent 自己设置里的回调，用来接收请求与回复，只在客户端回调打开时存在；关闭即一并移除。
- **上传**：从本仓库源码构建的应用不上传任何数据。
- **在 iPhone 与 Apple Watch 上访问数据**，经由发行版：
  - **同一局域网内**走 HTTPS，手机配对时固定这台 Mac 的证书。
  - **任何网络下，借助 Agent HUD Remote**，经由你自己的 iCloud。记录正文端到端加密，密钥在你的 iCloud 钥匙串里；只有提醒要显示的文字不在加密范围内，以便系统弹出提醒。
  - **运行中会话的实时活动**同样需要 Remote，经 Agent HUD 的中转服务交给 Apple 的推送服务。中转只保留转发所需的内容，即每台 Mac 最新的会话与每部手机的卡片，不保留账户身份；30 天没有请求的账户会被删除。

## 从源码构建

**环境要求：** Node.js 24+、pnpm、较新的 Rust 工具链（1.90+），以及你所在系统的 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git clone https://github.com/jazzenchen/agent-hud-open.git
cd agent-hud-open
pnpm install
pnpm run check
pnpm run build:macos -- --bundle
```

在 Windows 或 Linux 上改用 `build:windows` 或 `build:linux`；每条命令只为它所在的系统构建。Tauri 把应用写在 `target/release/bundle` 下。想在没有安装任何 agent 时试用，用 `--demo` 启动，它会用示例数据打开应用。

**修改过的 Tao。** HUD 在 macOS 上必须是一个不抢焦点的面板，不会把你正在用的应用切到后台；上游 Tao 目前还创建不了这种窗口（[tao#414](https://github.com/tauri-apps/tao/issues/414)）。在上游支持之前，工作区从[一个公开的分支](https://github.com/jazzenchen/tao/tree/agent-hud/nonactivating-panel)取 `tao`：在 0.37.1 之上只多一个小提交，在 `Cargo.toml` 里按提交固定，首次构建时由 Cargo 从 GitHub 拉取。改了什么、以后怎样换到新的 Tao 版本，见 [Tao 补丁](docs/tao-panel-patch.md)。

**预览版。** 这个构建正逐项对照原生 Swift 版 [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open)（原 agent-hud-open）的行为，见[对齐清单](docs/parity/README.md)。它暂不安装客户端回调，因此批准、提问与大部分回复仍留在各 agent 自己的提示里。

<details>
<summary><strong>开发</strong></summary>

| 命令 | 用途 |
| --- | --- |
| `pnpm run check` | 构建界面与 MCP 服务，检查平台边界与对齐清单，对 Rust 做类型检查 |
| `cargo test --workspace` | 运行 Rust 测试 |
| `pnpm run dev` | 在浏览器中以实时数据运行界面，地址 `http://127.0.0.1:1420`（先运行 `pnpm run engine:build`） |
| `pnpm run build:macos` / `build:windows` / `build:linux` | 为当前系统构建应用 |
| `pnpm --silent run mcp` | 通过 stdio 运行 MCP 服务 |

工作区结构、MCP 客户端配置与其他命令见[开发文档](docs/development.md)。代码放在哪里，按[贡献指南](CONTRIBUTING.md)：核心负责计算，界面只负责显示，每个系统的代码留在各自的文件里。

</details>

## 文档

[开发](docs/development.md) · [平台架构](docs/platform-architecture.md) · [平台构建](docs/platform-builds.md) · [账户](docs/accounts.md) · [客户端回调](docs/client-hooks.md) · [会话数据](docs/session-data.md) · [性能](docs/performance.md) · [对齐清单](docs/parity/README.md) · [Tao 补丁](docs/tao-panel-patch.md) · [第三方来源](docs/third-party.md) · [贡献指南](CONTRIBUTING.md)

## 许可证

[Apache-2.0](LICENSE)。界面组件、各厂商图标与 Clawd 的动画帧保留各自的声明，见[第三方来源](docs/third-party.md)。

## Star 历史

<a href="https://www.star-history.com/?repos=jazzenchen%2Fagent-hud-open&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&legend=top-left" />
 </picture>
</a>
