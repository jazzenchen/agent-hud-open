# Usage semantics

## Overview

How Agent HUD Open counts tokens, names quota windows, colors readings, spaces account requests and keeps readings. The rules apply to every provider and to the desktop, and hosts use the same numbers. Where each client's fields come from is in [providers](providers.md); running and terminal turns are in [session lifecycle](session-lifecycle.md).

## Model

| Kind | Contains |
| --- | --- |
| Cache write | Prompt tokens written to the prompt cache |
| Input | The other new prompt tokens |
| Reasoning | Output spent reasoning or thinking |
| Output | The other output |
| Cache read | Prompt tokens read back from the cache |

The five kinds (`TokenKind`, selected as `TokenDimensions`) are additive and never overlap. Logs count cache writes inside input and reasoning inside output; a log that does not tell them apart puts everything in input and output, and so do events recorded before the ledger split them, until their logs are read again. Charts, the heat map, model shares and session rows follow the selected kinds; the default is every kind but cache reads (`fresh`, shown as excluding cache reads), and `all` adds cache reads. The panel's per-session token figure leaves cache reads out too, and the statistics window's Sessions page lists the sessions active in the last seven days, under the day each last did something on, whatever range its Tokens page shows. A quota window is one row per window the service reports, with that window's own reset time and period; a reading is the last value of a window, balance or reset-credit count together with its observation time. An account is whose quota a window describes, identified by the provider's own user and workspace ids; a window's row id is the account id followed by the provider's window name (`account:<hash>/codex`).

## Rules

### Percentages, windows and presentation

- Every percentage shown is used, the convention of Claude Code's `/usage`; providers store the remaining share and the desktop converts.
- Rows come from the response, never from a template: `primary` is not assumed to mean 5 hours, and a window the service names by period (minutes, "7d", weekly, monthly, "MCP") keeps that label.
- One account's windows are shown once even when two programs share the account (Codex Desktop and CLI); usage recorded by one client is never duplicated into another client's account, and model token spend is shown separately from quota windows.
- Money keeps its currency and is never converted or turned into a percentage; API-billed clients (DeepSeek) show balance and estimated cost instead of windows, and estimates are labelled as estimates.
- Codex reset credits show the service's `availableCount`; the per-credit expiry list is supplementary and never derives the count.
- A missing reading is "—", not 0; zero is shown only when the service reported zero.
- Tokens are never converted into quota, and an unavailable quota is never inferred from token counts. Claude Code's session share is its share of the tokens in the current 5 h window times the window's utilization, given only to sessions that spent tokens in that window; every other session shows "—".
- One API response is counted once whatever the log layout; copies of one event from two files or two Macs merge, and distinct requests with identical counts are kept. Bar buckets are 15 min, 30 min, 1 h or 1 d on local quarter-hour, hour or calendar-day boundaries inside the exact half-open range; empty buckets keep their position and counts stay integers.
- A session's breakdown (`SessionUsage`) is read from the ledger: its own log, or its id for sources read whole, plus every log under the directory named like its log (sub-agents), by kind, model, 15-minute period and turn, with the number of calls. Its totals therefore exceed the session row, which leaves sub-agents out. A session is read again when its counts move or when the ledger has changed any log it is made of, its own or one under its sub-agents' directory, since its breakdown was read.
- A turn is a prompt in the session's own log and every call, sub-agents' included, until the next prompt; calls before the first prompt form a turn of their own, a prompt no call followed forms none, and a turn notes whether the client compacted the conversation during it. Claude Code, Codex and DeepSeek Harness logs mark their prompts; sessions of other clients have no turns. A breakdown keeps the newest 200 turns and counts all of them.
- The context is a call's input with cache writes and cache reads: the session's is its latest call's, a turn's its last call's, given only for logs that record every call. The context window is the one the log reports with the call (Codex), else the model's published window; a Claude model runs with 200K or 1M, so one missing from the catalog counts as 200K until this Mac has seen it hold more.
- A session's list price (`ModelCatalog`) is what its calls would cost at Anthropic's and OpenAI's published API prices, for Claude Code's and Codex's models: input, cache writes at the one-hour rate Claude Code writes with, cache reads, and output with reasoning, at OpenAI's long-context rates for prompts over 272K tokens. It is an estimate, not a charge; a call whose model has no list price leaves the session without one.

### Alert levels

| Reading | OK | Warning | Critical |
| --- | --- | --- | --- |
| Quota window | used < 70% | used ≥ 70% | used ≥ 90% |
| DeepSeek balance in CNY | > 10 | ≤ 10 | ≤ 0 |
| DeepSeek balance in USD | > 2 | ≤ 2 | ≤ 0 |
| Balance in another currency | > 0 | — | ≤ 0 |
| Account reported unavailable | — | — | always |

- The policy is fixed (`AlertPolicy`): nothing is stored, synchronized or configurable. A color describes the resource state of one reading; readings of different windows are never combined into one health score, and a color never indicates task progress.
- The glow shows one segment per enabled window that has a reading; windows without a reading stay out of it, and a paused or hidden glow is grey.
- Alerts (`QuotaAlertTracker`): the first reading of a window is a silent baseline; crossing 90% used, reaching zero, a forecast of exhaustion before the reset, and a confirmed reset each notify once. Readings older than 30 minutes stay visible but generate no alerts.
- Added usage resets (`ResetCreditTracker`): a rise in a signed-in account's Codex reset-credit count notifies once, with how many were added; the first reading of an account is a silent baseline, and using a credit or letting one expire is not an event. A reading with a notice or older than 30 minutes confirms nothing.
- The burn rate (`UsageAnalytics.burnRate`) is the recent pace with idle time included: consumption over the last hour of a 5h window, the last day of a weekly one and the last week of a monthly one, or over the whole observed series while that is shorter. A rise under 5 points is reading noise; a larger one is a reset and restarts the series. A series shorter than 15 minutes, or one hour for windows of a day or more, gives no estimate.
- The island shows these alerts (`IslandEventTracker`). Baselines start again at every launch, so readings present at start-up never alert; nothing is checked while collection is paused or the last refresh failed, and alerts that arrive while the glow is hidden are dropped.

### Accounts

- Readings, quota history, alert baselines and display settings are keyed by the account's window rows, so two accounts of one client never share a history or a burn rate.
- The account of a provider's latest successful reading is current; only current accounts join the glow, the menu-bar figure and alerts. A failed refresh keeps the current account, and a login without plan limits makes no account current.
- Other accounts keep their last reading and its observation time, greyed under the account's name, until they have not been seen for 30 days; their readings, rows and display settings then retire.
- Changing accounts is neither a reset nor an exhaustion: an account that becomes current again starts a new alert baseline.
- The first identified account takes over an unidentified row's position and display switch; a window that appears on a further account inherits the switch of the same window on another account. Quota history recorded before accounts were identified belongs to no account and feeds no burn rate.
- A login the provider does not identify is one account per client home directory, never merged with another home. A provider that must forget its accounts, for example after reading consent is withdrawn, retires their readings, rows and display settings at once.
- Codex reset credits belong to the account that reported them; native and Pi logins can make several accounts current at once. Billing-pool rows (Kimi, GLM, OpenCode Go) keep their pool ids as account ids.

### Collection cadence

Reads never run in parallel: the usage store runs one pass of source reads or one account step at a time, and a pass starts only after the previous one finished. Every account request is a metadata read: no model message is sent and no reset credit is consumed.

| Work | Cadence |
| --- | --- |
| One client's local logs | When a file under its data directories changes, when one of its live sessions reaches 120 s or a running turn 120 s or 5 minutes without an observation, and after its account step; passes start at most every 2 s |
| Local logs of a source that cannot name its directories | Every 5 s |
| Every client's local logs while the first index is being built | Every 2 s |
| Account readings: Claude Code engine `get_usage`, Codex `account/rateLimits/read`, DeepSeek balance, Antigravity, Cursor, Grok and GitHub Copilot quota, Cursor account usage events, and Kimi, GLM and OpenCode Go quota per billing pool | Per client: every minute while one of its turns runs, every 3 minutes while a session of its is live between turns, once more for work that finished since its last reading, and when one of its windows resets. Also when the panel, the menu bar menu or the statistics window opens, and at once when GitHub Copilot quota reading is switched on or off |

- A client nobody is using is not asked: its windows move only while its own work runs. A window whose reset has passed, a reading that names no window, and a client whose usage is the account's from every device it signs in on (Cursor and Codex, including Pi logins) keep the 5-minute interval.
- A known reset takes priority over the normal cadence and stays due until an account request has run at or after it, subject to the 60-second request spacing. An old window returned after that attempt retries on the normal cadence.
- Account steps run one provider after another, back to back for at most one second before local logs get their turn, so a slow request delays a poll by that request alone.
- A pass reads only the clients that signalled; the others keep their last result. Every local source is read again every 5 minutes, which catches a change a directory watch missed. Claude Code and Codex polls examine only the logs the watch reported changed, and list every log again every 5 minutes or after dropped events.
- A provider never repeats an account request within 60 s, whoever asks, and a failure waits as long as a success; it is reported as a source notice while the other sources keep working.

### Reading retention

- The last successful reading is kept with its observation time; a failed refresh keeps it and exposes the failure, and a restart restores it before the first poll.
- When a window's reset time has passed, the row keeps the last reading and its time. A reset is confirmed only by a new reading whose reset time moved forward or that shows the window full again; until then alert evaluation treats the deadline as pending and the row displays “Pending update”. Historical accounts show no live countdown. A successful Codex response replaces that account's complete window inventory, removing omitted windows; failures retain the old inventory.
- Kimi, GLM and OpenCode Go rows are retired — readings, cached rows and display settings — once a completed credential scan finds their credentials expired, removed or rejected; a temporary network failure retires nothing.
- Quota histories keep 30 days.
- A running session leaves the running indicator 120 s after its last source observation and stays in history without an invented end time.

## Code map

| Concept | Code |
| --- | --- |
| Token kinds and dimensions, bar buckets | `Sources/AgentHUDCore/Models/TokenKinds.swift`, `Sources/AgentHUDCore/Logic/ChartData.swift` |
| List prices, context windows | `Sources/AgentHUDCore/Models/ModelCatalog.swift` |
| Alert levels, status colors | `Sources/AgentHUDCore/Models/AgentThresholds.swift`, `Sources/AgentHUDCore/Logic/StatusLevel.swift` |
| Alert tracker, added usage resets, island events, forecast, reading age | `Sources/AgentHUDCore/Logic/QuotaAlerts.swift`, `ResetCreditGrants.swift`, `IslandEvents.swift`, `QuotaForecast.swift` |
| Event union, analytics, history retention | `Sources/AgentHUDCore/Store/UsageAggregation.swift`, `QuotaHistoryStore.swift`, `Sources/AgentHUDCore/Logic/UsageAnalytics.swift` |
| Session liveness, retained readings | `Sources/AgentHUDCore/Models/LiveSession.swift`, `Sources/AgentHUDCore/Providers/RetainedUsageProvider.swift` |
| Session breakdown | `Sources/AgentHUDCore/Models/SessionUsage.swift`, `Store/UsageLedger.swift`, `Providers/CombinedUsageProvider.swift` |
| Accounts, current and previous readings, settings migration | `Sources/AgentHUDCore/Models/ProviderAccount.swift`, `Sources/AgentHUDCore/Providers/RetainedUsageProvider.swift`, `Sources/AgentHUDCore/Store/SettingsStore.swift` |

## Related

[providers.md](providers.md) per-client fields and request intervals · [session-lifecycle.md](session-lifecycle.md) running and terminal turns · [architecture.md](architecture.md) design invariants
