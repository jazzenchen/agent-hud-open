# Parity ledger

The native Swift app, [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open) (formerly agent-hud-open), is the behavioral reference for this port. Below, **Open** means that app and **Next** means this repository. This ledger records, behavior by behavior, where Next matches it and where it does not. It replaces an earlier audit.

## Reference and method

- **Reference:** agent-hud-native-open commit `014524c` (main after PR #135, 2026-10-20; tree `fb694cc`). Later changes to agent-hud-native-open are not tracked until Next catches up.
- **Port:** this repository as of 2026-10-20, before its first commit. Next file and line references in the chunk reports point at that snapshot. The platform-adapter refactor of the same day moved some OS code (see [platform architecture](../platform-architecture.md)); behaviors did not change.
- **Method:** every Open source file (68,700 lines in `Sources/`) was read in full and compared with the Next code that implements the same behavior. Each behavior is classified:

| Status | Meaning |
| --- | --- |
| same | Next matches. |
| deliberate | An intended difference; see the list below. |
| better | Next differs and is clearly better; the reason is written in the row. |
| fixed | Was missing or different; fixed since the audit, with the date in the row. |
| missing | Open's behavior is absent. |
| differs | Next implements it differently. Treat as a bug unless a later decision marks it deliberate or better. |
| unclear | Could not be decided from the code alone; the row says what is needed. |

Deliberate differences:

- Decided 2026-10-22 (issue #3): Settings → General → Usage display can show consumed (the default) or remaining quota. Percentages, rings and bars follow this choice, including the menu bar and reminders; source readings, warning levels, burn rates and exhaustion forecasts remain based on consumed usage.

- The HUD has no glass material: every look is solid, with a transparency (decided 2026-10-21, after the React Bits / Liquid Glass look proved out of reach in the WebView; c07 #57).
- Next keeps its own settings and database, reads legacy preferences once and never writes Open's database. The one-time import of quota history reads `usage-ledger.sqlite` from the Application Support folder named by the `AgentHUDDataDirectory` key in the Info.plist of the installed `Agent HUD.app` (in `/Applications` or `~/Applications`); with no such key it reads Agent HUD Open's folder. An in-place update replaces that bundle with the app built on Next, so a product that updates an earlier app in place must declare the same key, or the import misses the earlier app's ledger.
- Demo mode only through `--demo`.
- The LAN token-usage and session service, which Open does not have.
- The Windows and Linux adapters.
- Decided 2026-10-20:
  - during the preview, client hooks default off and install only with a delegated fixed executable, so a preview build never rewrites existing client hooks;
  - the Settings window opens at launch with a Dock icon (since 2026-10-21 the HUD shows from the first run, as in Open; hiding it stays an extra choice);
  - Grok CLI and Grok Bot are two sources, not one Grok source with two clients;
  - sub-agent and child sessions nest under their parent. (This decision also said the parent counts only its own tokens; that part was reversed on 2026-10-21, below.)
- Decided 2026-10-21, reversing part of the last one: a session's tokens and cost include the sub-agents it started, at any depth, as in Open, wherever a session is shown (Sessions rows, day headers, the Today card, the HUD panel's session rows, the snapshot's sessions, the session page), and the session page shows the sub-agents' share. Totals that add every log (statistics, agent cards, quota windows) still count each sub-agent's log once. This is no longer a difference from Open.

Open is the floor, not the ceiling. Next must not do worse than Open; where it can do better, it should, and the reason is written down. Where neither is clearly better, Next follows Open. Behaviors are compared, not lines: data, interaction, timing and visuals.

## Totals

| Chunk | Open scope | Rows | same | deliberate | better | fixed | missing | differs | unclear |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| [c01](c01-claude-codex.md) | Claude, Codex, provider kit | 132 | 46 | 5 | 9 | 51 | 1 | 15 | 5 |
| [c02](c02-deepseek-catalog.md) | DeepSeek, provider catalog, window names | 43 | 10 | 0 | 1 | 5 | 3 | 21 | 3 |
| [c02a](c02a-grok-devin.md) | Grok CLI, Grok Bot, Devin | 55 | 18 | 3 | 0 | 2 | 11 | 20 | 1 |
| [c02b](c02b-antigravity-cursor.md) | Antigravity, Cursor | 51 | 21 | 1 | 2 | 5 | 6 | 16 | 0 |
| [c02c](c02c-buddy-copilot-qwen-zcode-hermes-openclaw.md) | CodeBuddy/WorkBuddy, Copilot, Qwen, ZCode, Hermes, OpenClaw | 70 | 29 | 0 | 1 | 7 | 4 | 26 | 3 |
| [c02d](c02d-openagents-additional.md) | Kimi, OpenCode/Go, GLM, Pi, additional sources | 57 | 16 | 2 | 0 | 4 | 6 | 28 | 1 |
| [c03](c03-hooks-store-ledger.md) | Hooks, permissions, collector, store, ledger | 104 | 44 | 6 | 6 | 4 | 8 | 28 | 8 |
| [c04](c04-models-logic.md) | Models, quota math, session phases, formatting, demo | 122 | 49 | 3 | 7 | 18 | 12 | 26 | 7 |
| [c05](c05-panel-mcp.md) | Panel layout, HUD style, MCP tools | 96 | 23 | 8 | 1 | 16 | 2 | 45 | 1 |
| [c06](c06-notch-surface.md) | Island, hover panel, Simple mode, HUD windows | 119 | 40 | 5 | 1 | 18 | 11 | 41 | 3 |
| [c07](c07-glow-alerts-theme.md) | Glow, alerts, permission cards, theme | 119 | 39 | 3 | 4 | 24 | 6 | 39 | 4 |
| [c08](c08-bots.md) | Bots and Clawd | 58 | 35 | 1 | 2 | 1 | 1 | 15 | 3 |
| [c09](c09-app-settings.md) | App lifecycle, Settings, menu bar | 90 | 25 | 6 | 6 | 7 | 9 | 32 | 5 |
| [c10](c10-statistics-debug.md) | Statistics, sessions window, debug, recording | 101 | 14 | 1 | 9 | 13 | 8 | 54 | 2 |
| **Total** | | **1,217** | **409** | **44** | **49** | **175** | **88** | **406** | **46** |

`pnpm run parity:ledger` recounts this table from the rows; `pnpm run check` fails when it is stale.

## What already matches exactly

- The price catalog: every model, region, rate, tier, alias and route (111 models).
- Burn rate, outlook order, alert thresholds, and the session quiet and abandoned limits.
- The bot engines: motion, moves, renderer, spec table, Clawd player, performer and baker, and the director, including curves and seeded random numbers.
- Most token arithmetic, the protobuf readers, and the Cursor and Grok Bot billing joins.

## Problems that cut across chunks

These account for most rows. Fixing each once fixes many rows.

1. **Session titles.** Fixed 2026-10-20. Readers record evidence (a name the client keeps, the first prompt, a client-specific last resort) and one resolver, `naming::session_title::display`, applies Open's rule and each client's fallbacks. Claude's custom and generated titles, and Copilot's, Kimi's and Grok CLI's title files, are now read, and Antigravity's non-ASCII titles decode. Still open: Codex sub-agent names, Hermes and OpenClaw client labels, and observer titles for placeholder sessions.
2. **English-only text.** Fixed 2026-10-20 for what people see: Rust writes its notices, statuses and card text in the app's language (`agent_hud_core::l10n`, as Open's `L10n`), Core publishes the resolved language so the windows follow it even where the web view does not know the system language, the remaining English-only labels in Settings, Statistics, Sessions and the HUD are bilingual, and the implementation notes and the migration page are gone. Internal errors that only reach logs or MCP callers stay in English. Forecast and sampling text is still built in English in TypeScript (#3).
3. **Logic in TypeScript.** Short window labels are fixed 2026-10-20: Core names every window's short form as Open does, checked against Open's own functions. Forecast text, status levels, queue membership, the mark director, cache-hit rate and turn reconstruction are still computed in the UI. Open keeps them in Core, and the TS versions already disagree with it.
4. **Silent data loss.** Open excludes malformed or unverifiable records and says so in a notice. Next mostly skips them without a word, or counts a bad value as 0.
5. **Launch behavior.** Partly fixed 2026-10-20: language and appearance default to the system's, and one copy runs at a time. The lock is the kind Open takes, and a copy that owns client hooks shares Open's lock file, so the two exclude each other. A second launch of the same app brings it forward, where Open only says it runs. The last account readings come back at launch from a restart copy, also written at quit. Still open: launch at login stays off during the preview.
6. **Serial collection reads.** Fixed 2026-10-20: account readings run one at a time and never beside a local pass; Settings republishes through the collector; a source toggle no longer scans on the request thread; the Claude Desktop index and WorkBuddy metering database are read only during a collection pass, not by queries. Like Open, the hook process's own origin lookup and the Antigravity approval observer run outside the app's collection pass.
7. **Pricing region.** Fixed 2026-10-20 for coding plans and DeepSeek: each client's models are priced on its platform, as Open's `PriceRegions` decides, and a model a platform does not sell has no price there. Still open: API keys configured straight to a China endpoint are not read. Money shows `¥` since the format slice.
8. **Sessions and Statistics.** Session costs show list prices since 2026-10-21 (a priced account's own estimate is still missing), the session page uses a different model (per message rather than per prompt), token numbers use a different format, model names read as Open names them since 2026-10-21, and Git worktrees are not grouped.
9. **Panel edits and MCP.** Settings changes fork a display off the shared layout, and the edit hold cannot be ended by the person. Since 2026-10-21 `draft_hud` refuses what Open refuses, with Open's sentences, keeps a proposal as what it changes (one that draws what is saved leaves no draft), uses the shared layout for a screen that follows it, and treats `from` as a whole proposal; `previous` is still a one-step snapshot in memory.
10. **Product decisions.** macOS notifications are gone since 2026-10-20 (Open is island-only); the "Play a sound" option is gone since 2026-10-21, as decided that day. Token history now expires after 31 days, as in Open (fixed 2026-10-20). Simple mode now hides from Mission Control as Open PR #135 does (2026-10-20). The recording and demo flags (`--record`, `--demo-script`, `--demo-clock`) are gone.

## Corrections to the earlier audit

The earlier audit listed some gaps that are in fact implemented, and missed others:

- Implemented: Cursor request-level billing sessions; Grok Bot spend, routine and billing associations; WorkBuddy renamed-database metering; WorkBuddy 429 ceilings.
- Confirmed gaps: Grok Bot transcript replicas; Grok CLI online quota and wallet; GLM-plan deduplication; OpenClaw SQLite archives; OpenCode JSON over 2 MiB; credit attribution (computed, then discarded).
- Not listed before: the Antigravity quota summary; Copilot, Kimi and Grok CLI titles; Kimi plans used without the Kimi CLI; quota-to-model linking for every source except Claude and Codex; the restart copy; the items above.

## Tests

[Shared fixtures](../../src/fixtures/parity/README.md) pin behaviors with values computed by Open's own code. Rust and TypeScript run the same cases; `decisions.json` records known gaps and improvements.


Most Open Core tests are data in, data out: JSON, JSONL or SQLite fixtures in, readings or sessions out. Each chunk report lists its tests and whether they can become shared fixtures. Those fixtures, run by both implementations, are how a fixed row stays fixed.
