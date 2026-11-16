# Agent HUD on macOS, Windows and Linux

What Agent HUD does on each system today. macOS is the main release; Windows and Linux are test builds (see each release's notes). For what each agent supports, see [Supported agents](https://agenthud.app/docs/en/agents/supported/); for agent messages, [Agents messaging each other](https://agenthud.app/docs/en/mcp/messaging/).

## How to read the table

- **✓** works in the release download.
- **Preview** is in the Windows or Linux test build, with the limits its release notes list.
- **—** is not available on that system.
- **Not verified** is described in the code or the release notes but has not been tried on that system yet.

A build from this repository can differ from the release download (Agent HUD Remote, for one, is release-only); a row says so where it does. Each row names its source: a release note, a README section or a docs page.

## Features

| Feature | macOS | Windows | Linux | Source |
| --- | :---: | :---: | :---: | --- |
| Install and updates | ✓ | Preview | Not verified | [README: Install](../README.md#install), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| HUD: Notch mode | ✓ | — | — | [README: On your screen](../README.md#on-your-screen) |
| HUD: Simple mode on any edge | ✓ | Preview | Not verified | [README: On your screen](../README.md#on-your-screen), [platform-builds.md](platform-builds.md#host-prerequisites-and-limitations) |
| Quotas, balances and tokens | ✓ | Preview | Not verified | [README: Built for every desktop](../README.md#built-for-every-desktop), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Approvals and questions on the HUD | ✓ | Preview | Not verified | [README: On your screen](../README.md#on-your-screen), [platform-builds.md](platform-builds.md#windows-machine-acceptance), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Reply reminders and Return to session | ✓ | Preview | Not verified | [README: On your screen](../README.md#on-your-screen), [Supported agents](../README.md#supported-agents), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Agent messages: send and receive | ✓ | Preview | Not verified | [agent-messages.md](agent-messages.md#agent-messages), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Agent messages: start a session | ✓ | Preview | Not verified | [agent-messages.md](agent-messages.md#starting-sessions) |
| HUD tools (MCP) | ✓ | Preview | Not verified | [README: Built-in MCP](../README.md#built-in-mcp), [platform-builds.md](platform-builds.md#windows-machine-acceptance) |
| Statistics window | ✓ | Preview | Not verified | [README: Know your token usage](../README.md#know-your-token-usage), [README: Built for every desktop](../README.md#built-for-every-desktop) |
| iPhone and Apple Watch over the local network | ✓ | Preview | — | [README: Built for every desktop](../README.md#built-for-every-desktop), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Agent HUD Remote (iCloud) | ✓ | — | — | [README: Agent HUD Remote](../README.md#agent-hud-remote), [Release notes](https://github.com/jazzenchen/agent-hud-open/releases) |
| Languages | ✓ | Preview | Not verified | [Release notes](https://github.com/jazzenchen/agent-hud-open/releases/tag/v0.6.4) |
