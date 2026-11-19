# Agent HUD on macOS, Windows and Linux

What Agent HUD documents for **[0.6.5][release]**, checked on 2026-11-16. macOS is the main release; Windows and Linux are test builds. For what each agent supports, see [Supported agents](https://agenthud.app/docs/en/agents/supported/); for agent messages, [Agents messaging each other](https://agenthud.app/docs/en/mcp/messaging/).

## How to read the table

- **✓** works in the release download and has been tried on that system.
- **Preview** is in the Windows or Linux test build, with the limits its release notes list. It does not mean the feature has been tried.
- **—** is not available on that system.
- **Not verified** is described in the code or the release notes but has not been tried on that system for this table.

The macOS column has been tried on the macOS release. The Windows and Linux columns are a source-based comparison, not a GUI acceptance report: **Preview; Not verified** means a documented test-build capability, not a failed test or a confirmed working workflow. The 0.6.5 release notes explicitly say the Linux packages have not been tested on a real Linux desktop.

The table describes the release downloads. A build from this repository can differ: it does not install client hooks, so approvals, questions and most replies stay in each agent's own prompt; Agent HUD Remote is release-only. See [Build from source][source-build]. Each row links versioned release notes, documentation or implementation; the live guides above may describe a newer version.

## Features

| Feature | macOS | Windows | Linux | Source |
| --- | :---: | :---: | :---: | --- |
| Install and updates | ✓; macOS 14+, Apple silicon or Intel; self-updates | Preview; Not verified; x64 unsigned installer; manual updates | Preview; Not verified; x86_64 unsigned AppImage or `.deb`; manual updates | [0.6.5 release][release], [Installer][installer] |
| HUD: Notch mode | ✓; hardware notch or a stand-in bar | Preview; Not verified; stand-in bar | Preview; Not verified; stand-in bar; same compositor limits as Simple mode | [Screen modes][screen], [Notch setting][notch-setting], [Linux HUD][linux-hud] |
| HUD: Simple mode on any edge | ✓ | Preview; Not verified | Preview; Not verified; Wayland needs wlr-layer-shell; X11 placement depends on the window manager | [Screen modes][screen], [0.6.5 release][release], [Platform builds][builds] |
| Quotas, balances and tokens | ✓; varies by agent | Preview; Not verified; no Claude desktop sign-in quota | Preview; Not verified; no Claude desktop sign-in quota | [Supported agents][agents], [0.6.5 release][release], [Shared readers][readers] |
| Approvals and questions on the HUD | ✓; supported clients and their setup required | Preview; Not verified; client and adapter dependent | Preview; Not verified; client and adapter dependent | [Screen requests][screen], [Supported agents][agents], [Platform builds][builds] |
| Reply reminders and Return to session | ✓; terminal-pane jumps need iTerm2 | Preview; Not verified; client and native navigation dependent | Preview; Not verified; client and native navigation dependent | [Screen requests and navigation][screen], [Supported agents][agents], [Platform adapters][adapters] |
| Agent messages: send and receive | ✓; supported clients and their setup required | Preview; Not verified; client and transport dependent | Preview; Not verified; client and transport dependent | [Messaging overview][messages-overview], [Delivery and client limits][messages], [Platform adapters][adapters] |
| Agent messages: start a session | ✓; Claude Code, Codex, Antigravity or DeepSeek Harness | Preview; Not verified; Codex starts use its desktop app | Preview; Not verified; available clients and commands required | [Messaging overview][messages-overview], [Starting sessions][starts] |
| HUD tools (MCP) | ✓; layout, appearance, usage, sessions and statistics | Preview; Not verified | Preview; Not verified | [HUD tools][hud-tools], [Local MCP and capture adapters][adapters] |
| Statistics window | ✓ | Preview; Not verified | Preview; Not verified | [Statistics][statistics], [Shared readers][readers] |
| iPhone and Apple Watch over the local network | ✓; paired devices on the same network | Preview; Not verified; paired iPhone; iPhone app 1.7.9+ | Not verified; release pairing is not documented; source sharing unavailable | [Supported hosts][desktop], [Mobile features][remote], [0.6.5 release][release], [Phone sharing adapter][adapters] |
| Agent HUD Remote (iCloud) | ✓; release-only; subscription required | — | — | [Remote and subscription features][remote], [0.6.5 release][release] |
| Languages | ✓; nine languages, or follow the system | Preview; Not verified; same language choices | Preview; Not verified; same language choices | [0.6.4 language addition][languages-release], [0.6.5 language choices][languages] |

## Limits to check before installing

- **Linux desktop:** gtk-layer-shell is required even on X11 (`libgtk-layer-shell0` on Debian/Ubuntu), and so is the tray's appindicator (`libayatana-appindicator3-1`); the `.deb` installs both, except 0.6.7's, which missed appindicator. GNOME on Wayland does not offer wlr-layer-shell, so the edge HUD is unavailable there. On X11, native HUD dragging is unsupported. See [0.6.5][release] and [Platform builds][builds].
- **Agents and messages:** a platform entry does not promise every client feature. Cursor, TRAE, WorkBuddy, ZCode, Qoder and Kimi Code normally receive a message only when a turn ends; an interactive Qoder CLI session can also receive while idle. These clients cannot be started through `start_session`. Pi and OpenCode can receive while idle; OpenCode 2 cannot send yet. The [versioned agent table][agents] and [delivery documentation][messages] give the remaining client limits and setup requirements.
- **Windows sessions:** 0.6.5 cannot check whether a Claude Code process still runs. If it exits without clearing its record, the displayed state can remain stale until Claude Code starts again. See the [Windows release notes][release].
- **Phone and watch:** local sharing and iCloud Remote are different. The release documents quotas, statistics and sessions on the same network without a subscription; conversations, messages, decisions and the watch-to-session workflow are subscription features. Mac sync requires iPhone app 1.7.2+; the Windows notes require 1.7.9+. The Linux source build cannot share with phones because it has no credential store; Linux release pairing is Not verified. iCloud Remote is macOS-only. See [Mobile features][remote], [Phone sharing adapter][adapters] and [0.6.5][release].
- **Languages:** English, Simplified Chinese, Traditional Chinese, Japanese, Korean, French, German, Spanish and Brazilian Portuguese, selected in Settings → General → Language. See the [release announcement][languages-release] and [0.6.5 choices][languages].

[release]: https://github.com/jazzenchen/agent-hud-open/releases/tag/v0.6.5
[installer]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/src/apps/installer/README.md
[source-build]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#build-from-source
[screen]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#on-your-screen
[agents]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#supported-agents
[messages-overview]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#let-your-agents-talk-to-each-other
[messages]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/docs/agent-messages.md#sending-into-a-session
[starts]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/docs/agent-messages.md#starting-sessions
[hud-tools]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#build-your-own-ui
[statistics]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#know-your-token-usage
[remote]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#agent-hud-remote
[builds]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/docs/platform-builds.md#host-prerequisites-and-limitations
[readers]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/docs/platform-builds.md#shared-readers-and-os-specific-operations
[adapters]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/docs/platform-architecture.md#where-operating-system-code-lives
[notch-setting]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/src/packages/app/src/settings/display/HUDSection.tsx#L42-L68
[linux-hud]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/src/crates/host/src/hud/linux.rs#L1-L8
[desktop]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/README.md#built-for-every-desktop
[languages-release]: https://github.com/jazzenchen/agent-hud-open/releases/tag/v0.6.4
[languages]: https://github.com/jazzenchen/agent-hud-open/blob/v0.6.5/src/packages/ui/src/format/language.ts#L10-L22
