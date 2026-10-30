<h1 align="center">Agent HUD</h1>

<p align="center">
  <strong>HUD for your agents.</strong><br>
  On your screen. On your phone. On your wrist.
</p>

<p align="center">
  <a href="https://agenthud.app/mac"><img src="docs/buttons/mac.svg" alt="Download Agent HUD for Mac" width="180" height="36"></a>
  <a href="https://apps.apple.com/app/id6812113619"><img src="docs/buttons/app-store.svg" alt="App Store: iPhone and Apple Watch" width="138" height="36"></a>
  <a href="#build-from-source"><img src="docs/buttons/source.svg" alt="Build Agent HUD from source" width="181" height="36"></a>
</p>

<p align="center">
  <a href="#build-from-source"><img src="docs/badges/macos.svg" alt="macOS 12+" height="22"></a>
  <a href="#built-for-every-desktop"><img src="docs/badges/windows-linux.svg" alt="Windows and Linux in preview" height="22"></a>
  <a href="docs/platform-architecture.md"><img src="docs/badges/rust-tauri.svg" alt="Rust and Tauri 2" height="22"></a>
  <a href="LICENSE"><img src="docs/badges/license.svg" alt="Apache-2.0 license" height="22"></a>
</p>

<p align="center">
  <strong>English</strong> · <a href="README.zh-CN.md">简体中文</a> · <a href="https://agenthud.app">Website</a> · <a href="#supported-agents">Supported agents</a> · <a href="#documentation">Documentation</a>
</p>

<p align="center">
  <img src="docs/videos/agent-hud-loop.webp" alt="A tour of Agent HUD at a MacBook's notch. The glow breathes in red, yellow and green quota segments. A hover opens the panel on Claude's and Codex's quotas, a DeepSeek balance, the last 24 hours of tokens and the sessions, and a click turns Claude's quotas into burn rates. Claude then asks to run the test suite and is answered with Allow once, and its reply arrives as a New reply reminder whose card offers Return to session" width="640">
</p>

Your agents work on your Mac. Agent HUD brings their progress to you. Check where they are at a glance, pick up a conversation, make the next decision: at your desk, and after you leave it.

## On your screen.

**Your agents, at a glance.** You write code, read docs or deal with something else while your agents keep working. The edge of your screen tells you who is still busy, who is done and who is waiting for you.

The details are close at hand, and what needs your approval is answered right there. Every agent's progress, gathered where one glance takes it in.

<p align="center">
  <img src="docs/videos/simple-mode-tour.webp" alt="A tour of Simple mode on the left edge of a display. Five agent logos sit in quota rings, then show percentages and a balance. Hovering a logo writes that agent's quotas on one line, and a click opens Claude's card with its limits, burn rates and the 24-hour token bars. A permission request is answered with Allow once, a New reply reminder opens with Return to session, and the HUD is dragged to the top edge and back" width="640"><br>
  <sub>Simple mode on a display's left edge, with Separate agents on · Sample data</sub>
</p>

- **Glow**: one colour per agent, from its most used quota: green under 70% used, yellow from 70%, red from 90%, with a warning before a quota runs out. It moves faster while an agent works.
- **Hover**: quotas with their resets or burn rates, balances, the last 24 hours of tokens, and the sessions, those waiting for you first.
- **Requests**: approve, deny or always allow, and answer an agent's questions, all on the HUD, the questions Claude asks in its desktop app included. Agent HUD never decides for you: a request you leave goes back to the agent after 10 minutes.
- **Jump back**: one click takes you to a session's thread, tab or terminal pane.

**Three modes.** Each display runs one: Notch by default where there is a notch, Dynamic elsewhere.

| Mode | On screen |
| --- | --- |
| **Notch** | At the notch, with the glow around it. |
| **Simple** | A small floating panel on any screen edge: each agent's logo in a ring of its most used quota, with the percentage beside it. |
| **Dynamic** | A row of agent logos on any screen edge, with the glow behind them. A logo becomes its bot while its agent works. |

## On your phone.

**Keep moving when decisions are needed.** Leave your computer and the work still goes with you. On iPhone, check progress, read replies and continue the conversation.

When an agent reaches something only you can decide, your phone shows you the context, and you answer the question or approve the next step. The task keeps going, and so does your day. iPhone Duo is supported.

<p align="center">
  <a href="docs/screenshots/iphone-home.webp"><img src="docs/screenshots/iphone-home.webp" alt="Agent HUD on iPhone, Home tab: quota cards for Claude and Codex with the share used and reset time of each window, and a DeepSeek account balance" width="150"></a> <a href="docs/screenshots/iphone-sessions.webp"><img src="docs/screenshots/iphone-sessions.webp" alt="Agent HUD on iPhone, Sessions tab: today's tokens by kind with their cost, then sessions with per-turn bars and context-window fill" width="150"></a> <a href="docs/screenshots/iphone-duo-stats.webp"><img src="docs/screenshots/iphone-duo-stats.webp" alt="Agent HUD on an unfolded iPhone Duo, Stats: tokens stacked by model left of the fold, and recent usage for today, the last 7 days and the last 30 days on the right" width="455"></a><br>
  <sub>Agent HUD for iPhone · Home and Sessions, and Stats on an unfolded iPhone Duo · Sample data</sub>
</p>

See [Agent HUD Remote](#agent-hud-remote).

## On your wrist.

**Start from your wrist, continue on iPhone.** An idea comes to you on a walk. Raise your wrist, say it, and it goes straight to the Mac at home. A thought caught in passing becomes a task in progress.

Read the latest reply on your watch. To discuss it or add detail, continue on iPhone. The idea starts on your wrist, and the conversation goes where you go.

<p align="center">
  <img src="docs/screenshots/watch-record.webp" alt="Apple Watch recording a new note: a waveform, 0:07, English, and a stop button" width="180"> <img src="docs/screenshots/watch-note.webp" alt="The note's text on Apple Watch, transcribed on iPhone, with the button Send to an agent" width="180"> <img src="docs/screenshots/watch-send.webp" alt="Send to: Claude and Codex, each its desktop app" width="180"> <img src="docs/screenshots/watch-replied.webp" alt="The same note marked Codex has replied, with the button View latest message" width="180"><br>
  <sub>A note spoken to Apple Watch becomes a Codex session on the Mac · Sample data</sub>
</p>

See [Agent HUD Remote](#agent-hud-remote).

## Built-in MCP.

**Your agents use Agent HUD too.** Its built-in MCP server gives them a channel to talk to each other, and the tools to build the UI you want.

### Let your agents talk to each other.

When Claude Code finishes a change, it can ask a Codex session to review it and get Codex's answer back, or start a new session and hand the task over. Agent HUD already sees every session, so it carries messages across clients.

You can watch who is talking to whom: the sender folds its message into a paper plane and flies it to the receiver's mark, and the answer flies back the same way.

- **A message, not the conversation**: only what the agent writes goes across; its session stays where it is. On one project, the project itself becomes the shared context, and the other agent reads the code for itself.
- **Claude Code, Codex, Antigravity and DeepSeek Harness**: each can message the others, or start a new Claude Code, Codex or Antigravity session.
- **Never in the way**: a message waits until its session is idle; it never interrupts a turn.
- **In the [release](https://agenthud.app/mac).**

### Build your own UI.

Tell an agent how you want to see things: *“Put Codex first and keep only the numbers I care about.”*

Through MCP, the agent rearranges your Mac HUD and changes what it shows and how it looks. The change appears at once as a preview and is kept only when you press **Save**, so the HUD fits the way you work.

- For Claude Code, Codex and any other MCP client.
- An agent can read the HUD, propose a layout or a look for any display, and check the result with a screenshot in which accounts are masked.
- The same tools answer questions about your usage, sessions and statistics, from the data the app already holds.

## Worth keeping on

### Know your token usage.

Know what your agents did, and how much they used. See your token use, look back at how it changed, and find where it went: which agents, models and sessions.

Quotas tell you how much is left. Token usage shows you how it was spent.

| Dimension | What you can track |
| --- | --- |
| **Time** | The last 5 hours, 24 hours, 7 days or 30 days, in bars from 15 minutes to a day. History stays on your computer for 30 days. |
| **Token&nbsp;kinds** | Input, output, reasoning, cache writes and cache reads, all together or any mix. |
| **Agents&nbsp;and&nbsp;models** | Each agent's share, tokens and cost, and the models behind them. |
| **Sessions** | Tokens with the sub-agents' part, cost, turns, calls, context fill and cache hits. Sub-agents count inside the session that started them. |
| **Turns&nbsp;and&nbsp;calls** | The new tokens of every turn, and each call with its model, context and the tools it asked for. |
| **Cache&nbsp;hits** | Today, the last 7 days and the last 30 days, each session, and each model within a session. |
| **Cost** | An estimate at each vendor's list price, from its China or international price list, marked ≈. It is not a charge. |
| **Activity** | The hours your agents worked over the last 7 days. |
| **Balances** | DeepSeek's spend per day and the day its balance runs out. |

### See your agents come to life.

Familiar agent logos become small bots that act out thinking, working and waiting. With a face on every state, one glance tells you what is going on.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/videos/agent-bots-row-dark.webp">
    <img src="docs/videos/agent-bots-row-light.webp" alt="The logos of the agents Agent HUD supports, in one row. A wave runs across them: each logo wakes into its bot and works, a few hop with an exclamation mark to ask for approval, and they smile when done and turn back into their logos" width="840">
  </picture>
</p>

They give the HUD a character of its own: your agents busy at work, signalling when they need you. Each bot keeps its logo's shape and colours and follows your pointer with its eyes; Claude Code's is Clawd. [See each bot's four states](docs/videos/agent-bots.webp).

### Small footprint. Light on your Mac.

A HUD runs all day, so staying light is part of the experience. Agent HUD keeps its CPU, memory and energy use down, and leaves your system to your agents and the work in front of you.

Measured on a real Mac, not claimed:

<!-- Numbers come from release builds measured as docs/performance.md describes. -->
| Measure | Idle | Busy | Made up of |
| --- | --- | --- | --- |
| CPU, share of one core | about&nbsp;7% | 7%–&#8288;28% | Agent HUD 6–11%, each display's HUD 0.5–7%, graphics and networking up to 5% |
| Memory | about&nbsp;240&nbsp;MB | 240–&#8288;260&nbsp;MB | Agent HUD's own process, as Activity Monitor shows it. Each display's HUD adds 70–170 MB, graphics 10–50 MB and networking 7 MB |
| Energy over an 8-hour day | about&nbsp;0.1&nbsp;Wh | 0.1–&#8288;2.3&nbsp;Wh | Agent HUD more than half, the HUDs and graphics the rest |

Idle: no agent working, one display. Busy: agents writing to their logs, one or two displays. Release builds on an Apple M1 Max, macOS 26, with about 8,000 session logs. [How it is measured](docs/performance.md#measured-footprint).

## Built for every desktop

Agent HUD is built with Tauri 2: a Rust core, a React interface, and a thin native layer for each operating system.

- **One core.** Reading your agents' records, counting tokens and working out quotas, prices and sessions happen once, in Rust, the same way on every system. The windows only display what the core computes.
- **Each system in its own files.** Paths, processes, credentials, hooks and windows are written once per system, so work on Windows or Linux never touches shared code.
- **macOS today; Windows and Linux in preview.** Both build from this source, but neither has been verified on real machines yet; what is still missing is under [Coming soon](#coming-soon).
- **iPhone and Apple Watch pair with a Mac.** Agent HUD Remote syncs through iCloud, so it is available on macOS only.

## Agent HUD Remote.

**Your agents, wherever you are.** Through your own iCloud, Agent HUD Remote keeps your iPhone and Apple Watch connected to the [release](https://agenthud.app/mac) on any network.

### What you'll get

When your iPhone and the release share a network, iPhone and Apple Watch show your Mac's quotas, token statistics and sessions without a subscription. Remote adds:

- **Any network**: your quotas, token statistics and sessions, away from home too.
- **Conversations**: read Claude Code, Codex, Antigravity and DeepSeek Harness sessions, and message Claude Code and Codex.
- **Decisions**: allow or deny a tool call, answer a question, approve a plan.
- **Your wrist**: an idea you say becomes a new session on your Mac, and the reply comes back to your watch.
- **Alerts and Live Activities**: alerts on iPhone and Apple Watch, and your running sessions on the Lock Screen and in the Dynamic Island.

| | iPhone | Apple Watch |
| --- | :---: | :---: |
| Quotas, balances and token statistics | ✓ | ✓ |
| Sessions and their tokens | ✓ | ✓ |
| Widgets on the Home Screen, Lock Screen and StandBy | ✓ | — |
| Several Macs | ✓ | ✓ |
| Conversations | ★ | ★ |
| Messages to Claude Code and Codex | ★ | ★ |
| A note that becomes a new session | ★ | ★ |
| Tool calls, questions and plans | ★ | ★ |
| Alerts | ★ | ★ |
| Running sessions as Live Activities | ★ | ★ |

<sub>✓ Free to use · ★ Needs a subscription</sub>

**Check out in the App Store**, inside the iPhone app: [China mainland](https://apps.apple.com/cn/app/agent-hud/id6812113619) · [International](https://apps.apple.com/us/app/agent-hud/id6812113619)

## Coming soon

- **Windows and Linux out of preview**, with client hooks on Windows, and Linux packages that can install them. [Contributions are welcome](CONTRIBUTING.md).
- **More agents**, beyond the 18 in the table below.
- **API keys billed by the token**: their usage, spend and balance, next to your plans' quotas.
- **UI prompt templates.** Ready-made prompts that reshape your HUD in one go.
- **A Gantt view of sub-agents.** On a session's page, each sub-agent of a turn in its own lane on one time axis: when it started, how long it ran, what it used, and what ran side by side.
- **Agents shape every screen**, not only the HUD, with image backgrounds and a customizable iPhone app.
- **More agents in the conversation**, beyond Claude Code, Codex, Antigravity and DeepSeek Harness.
- **Files both ways.** Your agents send files to your iPhone, and you send files from your iPhone to your agents.
- **Recordings straight to your agents.** A recording from your Apple Watch or iPhone reaches your agent as it is.
- **More of the HUD is your call**: what Simple mode's rings show (burn rate, time to reset or cost), and which agents may alert you.
- **More from each agent**: Grok CLI's quota and one click back, OpenClaw's live status and replies, Devin's live status.
- **Requests and replies on the HUD in builds from source**, once client hooks leave the preview.
- **HarmonyOS.** An app for HarmonyOS phones is in review: on the same network as the [release](https://agenthud.app/mac), your quotas, token statistics and sessions, with service cards on the home screen.

…and more.

## Supported agents

Install and sign in to the agents you want to follow; Agent HUD finds them by itself.

| Agent | Tokens and sessions | Quota | Live status | Approvals | Questions | Replies | One click back | Turns and calls |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **Claude&nbsp;Code** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Codex** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **DeepSeek&nbsp;Harness** | ✓ | Balance | ✓ | ✓ | ✓ | ✓ |  | ✓ |
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

Also: the **GLM Coding Plan** quota, and approvals for the **Qoder** builds. Codex approvals need its hook trusted in Codex, DeepSeek Harness approvals and questions need Agent HUD's plugin (Settings → Agent plugins), Copilot's quota needs your consent in Settings, and Devin's quotas come from its CLI's local cache. Cursor's tokens come from Cursor 3.27 or later. Grok Bot's bots run on Cursor's servers, which report each bot's daily tokens; an account merged into SuperGrok reports none, so only its sessions show. Grok Bot's weekly quota comes from the same servers when Cursor is signed in to the Bot's account, otherwise from the Bot's own cache, which it refreshes only while in use. An account merged into SuperGrok has one weekly limit for Cursor and Grok Bot, and Cursor's block shows it as Cursor does.

## Keep your data safe.

- **Read on your computer**: the records your agents already keep. Usage history stays in a local SQLite database.
- **Read from each vendor**: quotas, balances and Grok Bot's daily tokens, with the agent's existing sign-in or configured key. No read sends a message to a model, and credentials are never stored.
- **Written on your computer**: hooks in your agents' own settings, to receive requests and replies, only while client hooks are on. Turning them off removes them.
- **Uploaded**: nothing, by a build from this source.
- **Access your data from iPhone and Apple Watch**, through the release:
  - **On the same network**, over HTTPS, with the Mac's certificate pinned when the phone pairs.
  - **Anywhere, with Agent HUD Remote**, through your own iCloud. Record contents are end-to-end encrypted with keys in your iCloud Keychain; only the text an alert shows stays outside that encryption, so iOS can display it.
  - **Live Activities of running sessions**, also with Remote, pass through Agent HUD's relay to Apple's push service. The relay keeps only what forwarding needs, each Mac's latest sessions and each phone's card, with no account identity, and deletes an account it hasn't heard from in 30 days.

## Build from source

**Requirements:** Node.js 24+, pnpm, a current Rust toolchain (1.90+), and [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```sh
git clone https://github.com/jazzenchen/agent-hud-open.git
cd agent-hud-open
pnpm install
pnpm run check
pnpm run build:macos -- --bundle
```

Use `build:windows` or `build:linux` on those systems; each command builds only for the system it runs on. Tauri writes the app under `target/release/bundle`. To try the app without any agents, start it with `--demo`, which opens it on sample data.

**A patched Tao.** On macOS the HUD has to be a panel that never takes focus from the app you are using, and upstream Tao cannot create one yet ([tao#414](https://github.com/tauri-apps/tao/issues/414)). Until it can, the workspace takes `tao` from [a public fork](https://github.com/jazzenchen/tao/tree/agent-hud/nonactivating-panel) with one small commit on top of 0.37.1, pinned by revision in `Cargo.toml`; Cargo fetches it from GitHub on the first build. What the commit changes, and how to move it to a new Tao release: [Tao patch](docs/tao-panel-patch.md).

**Preview.** This build is checked against the native Swift app, [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open) (formerly agent-hud-open), behavior by behavior, in the [parity ledger](docs/parity/README.md). It does not install client hooks yet, so approvals, questions and most replies stay in each agent's own prompt.

<details>
<summary><strong>Development</strong></summary>

| Command | Purpose |
| --- | --- |
| `pnpm run check` | Build the interface and MCP server, check the platform boundary and the parity ledger, type-check Rust |
| `cargo test --workspace` | Run the Rust tests |
| `pnpm run dev` | Run the interface in a browser at `http://127.0.0.1:1420` on live data (run `pnpm run engine:build` first) |
| `pnpm run build:macos` / `build:windows` / `build:linux` | Build the app for the system you are on |
| `pnpm --silent run mcp` | Run the MCP server over stdio |

The workspace layout, the MCP client configuration and the other commands are in [development](docs/development.md). Code goes where [Contributing](CONTRIBUTING.md) says: core computes, the interface displays, and each system's code stays in its own files.

</details>

## Documentation

[Development](docs/development.md) · [Platform architecture](docs/platform-architecture.md) · [Platform builds](docs/platform-builds.md) · [Accounts](docs/accounts.md) · [Client hooks](docs/client-hooks.md) · [Session data](docs/session-data.md) · [Performance](docs/performance.md) · [Parity ledger](docs/parity/README.md) · [Tao patch](docs/tao-panel-patch.md) · [Third-party sources](docs/third-party.md) · [Contributing](CONTRIBUTING.md)

## License

[Apache-2.0](LICENSE). Interface components, provider artwork and Clawd's sprites keep their own notices; see [third-party sources](docs/third-party.md).

## Star History

<a href="https://www.star-history.com/?repos=jazzenchen%2Fagent-hud-open&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jazzenchen/agent-hud-open&type=date&legend=top-left" />
 </picture>
</a>
