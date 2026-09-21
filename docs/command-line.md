# Command line

## Overview

Launch options, read-only probes and adapter commands of the standalone application. The same switches work on `build/Agent HUD Open.app/Contents/MacOS/Agent HUD Open` (after `make build`), on `swift run AgentHUDOpen`, and through Launch Services as `open "build/Agent HUD Open.app" --args …`. Unknown switches are ignored; probes and adapter commands run before the application object exists and never show a window. Applications that embed the libraries document their own parameters.

## Launch and display

| Switch | Effect |
| --- | --- |
| `--demo` | Use the design's sample data (`DemoUsageProvider`, `DemoData.agents`) instead of installed clients. Preferences live in the separate `app.agenthud.open.demo` defaults domain, no report cache is written, and no adapters are installed. |
| `--lang zh\|en\|system` | Force the interface language for this launch (`zh`, `zh-hans` and `cn` select Simplified Chinese). The choice is saved to the preferences in use. |
| `--show-settings` | Open the settings window after start. |
| `--show-stats` | Open the statistics window after start. |
| `--open-panel` | Start with the notch panel expanded. |
| `--show-onboarding` | Show the first-launch window even when onboarding is complete. |
| `--reset-defaults` | Remove the application's stored preferences before starting. |
| `--snapshot <dir>` | Render every screen at 2× to PNG files in `<dir>` from sample data, then quit. `AGENTHUD_SNAPSHOT_PREFIX=<name>` limits rendering to snapshots whose name starts with the prefix, for example `settings-`. |

`make demo` runs `--demo --show-settings`; `make snapshot` runs `--snapshot build/snapshots` (override the directory with `SNAPSHOT_DIR=…`).

## Read-only probes

| Command | Output |
| --- | --- |
| `--probe` | One real account refresh (48 h of history) followed by one report; prints `Quota windows: n; sessions: n; live: n; billing accounts: n` and exits 0, or prints the error and exits 1. It issues the same provider requests as the running application and nothing else. |
| `--probe-open-agents` | Indexes the last seven days of local OpenCode, Kimi and Pi sessions and prints, per client, the session count, running count, distinct usage events, In / Out / Cache totals and the read status. No network requests, no transcript text, no credentials in the output. |
| `AGENT_HUD_PROBE_ADDITIONAL=1 swift test --filter AdditionalProviderTests/testInstalledSourcesReadOnlyProbe` | Read-only probe of the installed Antigravity, Cursor and Grok sources from the test suite; the test is skipped unless the variable is set. |

## Adapter commands

| Command | Effect |
| --- | --- |
| `--install-pi-observer` | Write or update the Agent HUD extension `extensions/agent-hud.ts` under the Pi directory (`PI_CODING_AGENT_DIR`, default `~/.pi/agent`), then exit. Existing Pi sessions need `/reload` once. A same-named file that is not Agent HUD's is left alone and the command fails. |
| `--attention-hook claude` | The handler Claude Code invokes for a notification: reads the payload from standard input, records the session's pending request, prints `{}` and exits 0 even when recording fails. |
| `--permission-hook claude\|codex\|qoder\|qoderCN\|qoderWork\|codebuddy\|workbuddy\|zcode\|qwen` | The handler the client invokes when it is about to ask whether a tool may run: forwards the payload to the running application and waits, prints back the decision the user gave, and exits 0. Codex CLI and Desktop share the `codex` handler. Prints nothing — leaving the client's own permission flow untouched — when the application is not running, the payload cannot be read, or nobody answers before the client stops waiting. It never initializes the interface or queries an account. |
| `--install-completion-hook antigravity\|cursor\|copilot\|codebuddy\|qwen` | Register this executable as the client's stop-hook handler, replacing a handler that belongs to another installation. Other hooks in the client's configuration are preserved. |
| `--completion-hook antigravity\|cursor\|copilot\|codebuddy\|qwen` | The handler the clients invoke: reads the hook payload from standard input, stores a completion record when the payload describes a successful stop, prints `{"decision":"stop"}` for Antigravity or `{}` for the others, and exits 0 even when recording fails. It never initializes the interface or queries an account. |

Normal start-up already runs `SessionObservers.configure(executable:enabled:)` for installed clients with the `clientHooks` setting — installing when it is on, removing this installation's handlers when it is off — which also installs Claude Code's notification hook and each supported client's approval hook ([approvals](hud.md#approvals)). Codex requires the user to review and trust a new or changed hook through `/hooks` before it runs; the HUD does not change hook trust or enable disabled hooks. The install commands exist for a first setup without launching the application and for taking a hook over from another installation ([notification hook](session-lifecycle.md#notification-hook), [completion hooks](session-lifecycle.md#completion-hooks)).

## Environment

| Variable | Meaning |
| --- | --- |
| `SWIFT_SCRATCH_PATH` | Passed to `swift build` as `--scratch-path` by the build script |
| `AGENTHUD_SNAPSHOT_PREFIX` | Limits `--snapshot` to snapshots whose name starts with the prefix |
| `AGENT_HUD_PROBE_ADDITIONAL` | Enables the read-only probe test above |
| Client home overrides and provider keys (`CODEX_HOME`, `DSH_HOME`, `PI_CODING_AGENT_DIR`, …) | Read from the process environment ([providers](providers.md)). An application started from Finder or Launch Services inherits the login session's environment, not the exports of a terminal shell. |

## Code map

| Concept | Code |
| --- | --- |
| Switch parsing | `Sources/AgentHUDDesktop/App/LaunchOptions.swift` |
| Probes, adapter commands, hook handler | `Sources/AgentHUDOpenApp/main.swift` |
| Snapshot rendering | `Sources/AgentHUDDesktop/Debug/SnapshotRunner.swift` |
| Real-window interaction tests | `Tests/AgentHUDDesktopTests/IslandAnimationTests.swift`, `IslandHoverTests.swift`, `AgentSettingsInteractionTests.swift` |
| Build script and targets | `scripts/build-app.sh`, `Makefile` |

## Related

[session-lifecycle.md](session-lifecycle.md) completion hooks and the Pi observer · [providers.md](providers.md) per-client environment variables · [architecture.md](architecture.md) host integration
