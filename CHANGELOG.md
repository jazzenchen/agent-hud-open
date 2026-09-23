# Changelog

Releases of Agent HUD Open. A version is a git tag `vX.Y.Z` on `main`; `CFBundleShortVersionString` in `scripts/build-app.sh` carries the same number. Each entry lists what changed for people using the application and, under **Host API**, what changed for applications that embed `AgentHUDCore` and `AgentHUDDesktop`. Dates are tag dates.

## 0.4.21 — 2026-09-23

- Each session has its own page in the statistics window: agent, client, project, state and duration; its tokens with sub-agents included, the cache hit rate, the input of its latest call, its calls and its quota share or estimated cost; its tokens over its own span, stacked by model; its models; and the agent's last reply. A row of the session card, a session row of the panel and a completed turn on the island open it, and the back button returns to the overview.
- A session's tokens on its page include the logs of its sub-agents, which Claude Code keeps in the directory named after the session's log. The session list still counts the session's own log only.
- A quota event on the island opens the statistics window on its window's tile, scrolled into view and outlined for a moment; it used to open the window without pointing anywhere.
- Each provider in the panel has a button that switches its rows between quota, burn rate and token rate. Quota is the default. Burn rate shows the points used per hour and the use projected at the reset, or the date and time the window runs out before it; token rate shows tokens per hour and per day over the part of the current cycle this Mac observed.
- Host API: `SessionUsage`, `SessionUsageRequest`, `UsageReport.sessionUsage` and `UsageLedger.sessionUsage(_:)`; `UsageStore.focusedSessionID`, `focusedSession`, `sessionUsage(_:)`, `sessionMessage(_:)`, `sessionColumns(_:usage:)` and `quotaTokensPerHour(for:)`; `ChartData.tokenBars(usage:agentIds:interval:bucketSize:calendar:dimensions:)` and `ChartData.bucketSize(spanning:)`. The usage ledger adds an index on contribution keys the first time this version opens it.

## 0.4.20 — 2026-09-22

- Antigravity completions are recorded. agy ends a finished turn with `terminationReason` `NO_TOOL_CALL`, not the `model_stop` its hook guide shows, so no Antigravity completion was ever recorded. Its `executionNum` is 0 on every turn, so the turn is the callback time, as for GitHub Copilot CLI.

## 0.4.19 — 2026-09-22

- Codex CLI and Desktop, CodeBuddy (2.97 or later), WorkBuddy, ZCode and Qwen Code can be answered on the HUD, with deny and allow once. ZCode's `hooks.enabled` is set only when absent, and a rewritten ZCode configuration keeps its file permissions. A multi-edit shows its file and its first change.
- Qwen Code is a new client: usage from its `qwen-code.api_response` telemetry, counted once per line, with `/branch` copies skipped; completions through its Stop hook; no quota.
- Settings → General → Client hooks, on by default, switches every handler Agent HUD keeps in the clients' own settings. Switching it off asks first and names what stops working, then removes this installation's handlers; start-up adds none back.
- Settings → General → Wait for an answer, 10 minutes unless 1, 3, 5, 30 or 60 is chosen. An unanswered request then goes back to the client's own prompt. The HUD keeps the time itself, so no client's settings are rewritten and a new value applies to requests already waiting.
- A question Claude Code asks is answered on the island: each question with its offered answers, several where it allows them, and a field for your own words, one question at a time and sent together after the last. Any question can be skipped, and Claude Code hears which were left open; nothing on a question card refuses the call.
- The field is the only thing on the island that takes the keyboard, and only when clicked: a card that arrives never catches keys typed elsewhere. The keyboard goes back to the app in front when the answer is sent, Escape is pressed or another window is clicked, and the island stays open while it is being typed into.
- A plan Claude Code asks to have approved is not answered on the island. The card names it, shows its opening lines and points to Claude Code, whose own dialog carries the choices about how to go on; it can be put away without an answer.
- A request answered in Claude Code's own dialog, in the terminal or the desktop app, leaves the island as soon as the session record shows its result. Claude Code keeps its hook running after its own dialog is answered, so such a card used to stay until the wait ran out. An approved command's result is written when the command finishes, so its card stays until then.
- Questions and plan approvals from Qoder, CodeBuddy, WorkBuddy, ZCode and Qwen Code stay in the client's own dialog: those clients ignore an answer sent back, act on one without the user's reply, or are not known to read one.
- Host API: `SessionObservers.configure(executable:enabled:home:)` replaces `configure(executable:)`; hosts pass `Settings.clientHooks`. New: `Settings.clientHooks`, `Settings.approvalWaitMinutes`, `PermissionRequests.holdTime`, `PermissionQuestion`, `PermissionRequest.questions`, `isQuestion` and `isPlan`, `PermissionDecision.answer` and `.leave`, and `PermissionDecision.response(for: PermissionRequest)`, which `PermissionRequests.resolve` now uses.

## 0.4.18 — 2026-09-21

- Answering the last request leaves the island as it was, instead of sliding the usage panel under a pointer that was aiming at a button. A request answered as a row inside the panel still leaves the panel where it was.
- A request card clears the notch again. Its top inset was taken from the usage panel, which is wide enough that its first row sits beside a notch; a card is narrower and sits squarely under one, so the number comes from the screen's own silhouette. A screenshot cannot show this — macOS does not draw the notch into one — so it looked correct in every capture.
- Opening a shorter request, or answering one, no longer closes the island under the pointer. The card shrinks to its content as before, but the window keeps a transparent surface under the pointer until it leaves, so the pointer never ends up below the card it is still using.
- One request looks like a queue of one: same width, same header, same card. The width no longer changes as requests arrive and are answered.
- A Codex account that gains a usage reset says so. When the count of available resets rises between two readings, the island shows how many were added, how many the account holds now and when the first of them expires; hovering opens the details and a click opens the statistics. The first reading of an account, signing back in, using a reset and letting one expire are all silent, and a reading with a notice or older than 30 minutes confirms nothing.
- The burn rate follows recent use, idle time included: the last hour of a 5h window, the last day of a weekly one and the last week of a monthly one, or the whole observed series while that is shorter. A quiet start of the week no longer hides a heavy day. Readings wobble by a point or two between queries, and any rise used to be taken for a reset, which restarted the series and left a weekly window with hundreds of readings saying "Insufficient data"; only a rise of five points or more is a reset now.
- Host API: `ResetCreditTracker` and `ResetCreditGrant` in `AgentHUDCore`, and `IslandEventTracker.Update.resetCreditGrants`, which carries the added credits when the provider lists them so that a host can name the event the same way on every Mac.

## 0.4.17 — 2026-09-20

- A client that stops to ask whether a tool may run can be answered from the HUD. The request arrives on the island and stays there until it is settled, where every other event expires after a few seconds; hovering opens it, with the file or command it wants, the folder it runs in and, for an edit, the lines it would change. Deny and allow once are always offered, and a third answer appears when the client itself suggested a rule, which the HUD echoes back untouched rather than composing one of its own. Several requests stack into a queue, oldest open, any line of it openable, and answering one hands over to whichever has waited longest.
- Saying nothing stays available and costs nothing: a HUD that is closed, paused or simply not looked at leaves the client's own permission flow exactly as it would be with no hook installed, and a client that gives up — answered in its terminal, timed out, killed — takes its request off the island by itself. Nothing is ever answered on the user's behalf.
- Answering does not take the terminal's focus, because the island never becomes the key window; it is also why the card has no keyboard shortcuts.
- The clients that carry Claude Code's hook schema are covered: Claude Code and the Qoder, Qoder CN and QoderWork builds. Qoder's mark is bundled, and a client whose artwork is not bundled now draws a lettered badge instead of nothing at all.
- Demo mode shows the queue, so the feature can be seen without a client waiting behind it.
- Host API: `PermissionRequests`, `PermissionRequest`, `PermissionDecision`, `PermissionHooks` and `PermissionHookClient` in `AgentHUDCore`; `DesktopApplication` opens the channel on start and closes it on stop, and seeds the demo's requests in demo mode.

## 0.4.16 — 2026-09-20

- A logo queue also shows the agents you have actually been using: any vendor that ran in the last day joins the watched ones, most recently used first, and leaves again a day after its last turn. A client whose quota you do not follow was invisible on the HUD however much you ran it. A vendor whose Live status is off never arrives this way, since that switch is what says its runs may be reported at all.
- Host API: `UsageStore.queueVendors` and `queueRecency`.

## 0.4.15 — 2026-09-20

- A logo queue's marks bob again while their agent works. A session names the model it spends, not the quota window it belongs to, so comparing the two ids found a match only in the demo, where they happen to be equal: on a real Mac no mark ever moved. The vendor behind a session is resolved instead, by the same liveness the panel ranks sessions with.
- Codex and ChatGPT are one mark in the queue rather than two identical ones. They share OpenAI's artwork, and a second copy of it took a place in the row while telling a glance nothing; whichever of them is running bobs the mark they share.
- Host API: `UsageStore.workingVendors`.

## 0.4.14 — 2026-09-20

- Every display gets its own HUD, set on its own. A screen with no notch stops drawing a bar pretending to have one: it can instead show the watched agents' own logos in a row, with the glow behind them as a backdrop rather than a rim around a shape. A mark bobs while its agent has work running and holds still otherwise, so motion means one thing. The marks can be hidden, which leaves the backdrop alone, still where they would have been. The queue takes no mouse events while collapsed, so clicks reach the menu bar and the window under it, and hovering can be asked to take Option as well. Opening the panel leaves the field behind rather than wrapping it around the panel: only a notch is rimmed. An event is shown once, on the screen the pointer is on.
- The glow is set per display too. A notch wanting a rim and an external display's queue wanting a curtain no longer share a style, a reach or a speed; a display with none of its own follows the default, so a new screen needs no setting up.
- The glow's falloff is two settings instead of one, both counted in rows: how many keep full strength, and how many it fades away over. One decay length could only make a glow that starts dropping the moment it leaves the rim, and a backdrop wants a band held under the marks. The fade is Gaussian, which leaves the solid rows with no slope at all — an exponential starts at its steepest, and that shows as a crease where the two meet.
- Every glow effect has a speed, not only breathing: scan, ripple, flow, boot and shimmer scale to the period the user picks and keep the proportions they were tuned with. The glow no longer stops when nothing is running — it switches to a longer idle period, sampled at a lower frame rate — so a resting HUD reads as alive rather than dead.
- The character styles no longer come out in stripes. A glow's strength depends only on distance from the island, so every cell in a row picked the same glyph; the level is dithered by the cell's place in the Bayer matrix, which keeps the average density and breaks up the stripe.
- The HUD's drawing shares one budget across every display rather than spending one per screen, so a second display costs frames rather than CPU. Across two displays the grid styles fall from 12.3% of a core to 8.1%, and the heaviest settings the sliders allow from 17.5% to 8.9%. A blurred glow never enters the frame loop at all.
- The display preview shows the mode and the measurements of the screen it is set for, rather than a stand-in, and the sliders read in what they produce: rows of glow, points of logo.
- The panel's session list is ordered by what each session last did — a prompt, a reply, a tool result or an approval request — newest first, rather than putting every live session above every finished one. A running session nothing has been heard from for half an hour sits below one that just answered.
- Codex quota is read through Pi's own ChatGPT sign-in as well, and a ChatGPT account signed in from several clients is one account: its credits and its reset belong to the account rather than to the client that reported them. The agents page lists one summary per account for the same reason.
- A window whose reset has arrived keeps asking to be read until a reading has actually run, instead of losing the refresh it was scheduled for the moment the deadline passes; until one has, the row says it is waiting for an update. A quota read that failed no longer passes for a healthy current account — the account carries the notice it came back with.
- Host API: `HUDMode`, `HUDEdge` and `ScreenPlacement`, with `Settings.screens` keyed by a display's UUID; `GlowSettings` with `Settings.glow`, `screenGlow` and `glow(on:)`, replacing the flat glow fields as the model (the old names remain as accessors onto the default); `Settings.requiresOptionToOpen` and `placement(on:hasNotch:)`; `GlowPattern.core` and `fade` in place of `spread`, with `GlowMatrix.strength(cells:core:fade:)` and `reach(pitch:core:fade:)`; `GlowMotion.basePeriod(_:)` and `time(_:since:period:)`, with `gain` no longer taking `breathSeconds`; `GlowGeometry.fitted(within:)`; `GlowAppearance.resolve(levels:paused:anyAgentActive:glow:light:)` and `suppressed()`; `UsageStore.glowAppearance(light:on:)`; `StatusLevel.severity` and `worse(_:_:)`; `AccountObservation.quotaNotice` and `resetCredits`; `AgentRow.resetLabel(now:compact:)`; `LiveSession.lastEvent(turnAt:)`.

## 0.4.13 — 2026-09-18

- Shared artwork and the blur context are isolated so the island draws correctly under strict concurrency.

## 0.4.12 — 2026-09-18

- DeepSeek Harness sessions in formats 2 and 3 are read instead of reported as unsupported, including the generations Harness keeps beside an upgraded log, which count once from the newest readable one. Each settled message and each failed attempt counts as one attempt, taking its usage from the embedded stream, and a seeded fork's history is cut at its own tagged marker rather than at one copied from an ancestor.
- A Codex session is named after its first own prompt again. Current rollouts record a prompt only as a completed `UserMessage` item, so sessions that had no thread name showed their folder instead; rollouts already read are read again for it.

## 0.4.11 — 2026-09-17

- A running turn keeps the running indicator however quiet its log goes: one tool call can take minutes without writing a line, so only an end recorded by the client, an interruption, evidence that the client is gone, or thirty minutes of silence ends it. A source that never says what its turn is doing keeps the 120-second freshness rule, and DeepSeek keeps its process-table evidence.
- The island's session line answers what is running rather than what a range contains: every running session, three at most with the rest as a count, and the three that ended most recently when none is running. A turn blocked on the user is a running turn and wears the warning colour in both session lists.
- Quota is read when a client's own work moves it rather than every five minutes: every minute while one of its turns runs, every three minutes while a session of its is live between turns, once more for work that finished since its last reading, and when one of its windows resets. A client nobody is using is not asked at all. A window whose reset has passed, a reading that names no window, and a client whose usage is the account's from every device it signs in on keep the five-minute interval, since quiet says nothing about those.
- Opening the panel, the menu bar menu or the statistics window reads every account, so a sign-in made while a client sat quiet shows as soon as someone looks instead of waiting for the next sweep. A provider still never repeats an account request within 60 s, and every local source is still read every five minutes, which catches a file event the directory watch missed.
- Host API: `UsageRefresh.abandonedTurnTimeout`, `runningAccountInterval` and `liveAccountInterval`; `LiveSession.isLive(at:)`; `UsageStore.sessionState(_:)`, `isSessionWaiting(_:)` and `refreshAccounts()`; `UsageProvider.accountChecks(since:now:)` and `seesLocalWork`, both with defaults that keep the five-minute interval for a provider that says nothing.

## 0.4.10 — 2026-09-16

- Claude Code's notification hook is installed only for the notification types that mean the agent needs the user (`permission_prompt`, `agent_needs_input`), so a sign-in or quota notice never reads as a pending approval. Which kind of attention it is still comes from the transcript, and the transcript is still what says the request was answered.

## 0.4.9 — 2026-09-16

- Collection waits for signals instead of polling: a client's logs are read when a file under its data directories changes, when a live session or running turn ages past 120 s or 5 minutes, or after its account step, and a read covers only the clients that signalled. Nothing is read while every client is quiet, apart from the five-minute account sweep.
- A session says what it is waiting for and what the agent last answered. The turn's message is the latest visible assistant text, read up to 2 KB, kept only while the application runs and never written to the ledger; Claude Code's notification hook reports a turn blocked on the user. Whether that is a pending approval or an unanswered prompt comes from the transcript, never from the wording of a message, and a request is answered as soon as a newer transcript line arrives.
- Agent HUD installs Claude Code's notification hook in `~/.claude/settings.json` when Claude Code is present, the same way it installs the other clients' stop hooks; a machine without Claude Code is left untouched, and `--attention-hook claude` is the handler it registers.
- A Claude Code that is signed out says so beside its stale quota rows instead of showing only how long ago they were read. Its engine reports no plan limits both when signed out and when running on an API key, and `claude auth status` tells the two apart.
- Host API: `UsageSource`, `UsageProvider.sources`, `fetchUsage(agents:historyHours:sources:)` and `sourceChecks()` with defaults for a provider that does not split itself; `UsageRefresh.readSpacing` and `liveThreshold`; `UsageStore.observeChanges(_:)` with `UsageChanges` and `UsageChangeObservation`; `UsageRefresh.pollInterval` now applies only to sources without directories; `SessionTurn` gains the `waitingForApproval` state and a `message`; `AttentionHooks` with `Source`, `Event`, `record`, `read`, `configure` and `isActive`; `ClaudeDataError.signedOut`; `ClaudeEngineUsageClient.isSignedIn()`.

## 0.4.8 — 2026-09-15

- Lower idle CPU: clock ticks that change nothing on the island skip its layout, and the statistics window and forecast hover popups are created the first time they are shown.
- A poll reads less of the ledger: the hourly quota history, which no screen used, is gone, and the heatmap and weekly token share are derived from the stored usage buckets.
- Chinese interface: the notch is called 灵动岛 throughout, and vendor plan badges and descriptions say 套餐 / Plan instead of 订阅 / Subscription.
- `--snapshot` no longer runs the built-in island animation, hover and agent settings checks, which are XCTests now, and no longer writes the island forecast hover images.
- The restart cache written by 0.4.8 cannot be read by earlier versions; after a downgrade the first launch starts without the previous report until collection completes.
- Host API: `UsageStore(provider:settings:accessAllowed:hooks:)` with `UsageCollectionHooks(historyHours:publish:merge:)` and `UsageStore.remerge()`, collection scheduling moves into an internal collector; `DesktopMenuAction`, the `additionalMenuActions:` parameter and `DesktopSettingsPage.heading` are removed; `SettingsSection`, `Theme`, `Font.ui`, `Font.tabular`, `HostedWindowController` and `SourceDetector` become internal; `HistorySample`, `UsageReport.history`, `history(for:)`, `activity`, `insights` and `subscriptionType`, `UsageInsights.weeklyShare`, `windowSessionCount`, `windowUsedPct` and `empty`, and `ActivityGrid.empty` with its `Codable` conformance are removed; `UsageAnalytics.hourlyHistory`, `hourStart` and `weeklyShare`, `UsageAggregation.historyUnion` and `eventUnion`, `ChartData.remainingPath`, `usedPath` and `bucketed`, `BurnRate.estimate`, the `DemoSeries` candle, series, line seed and activity members, the `calendar:` parameter of `ClaudeCodeProvider.init`, `UsageLedger.bucketRevision`, `TranscriptSession.UsageEvent`, `L10n.vendorLabel`, `UsageStore.primaryRow`, `quotaUpdatedAt`, `weeklyByVendor`, `minRemainingPct` and `primaryInsights`, `SettingsStore.resetOnboarding()`, `Countdown.updatedLabel`, `GlowGeometry.visibleHeight`, `SourceStatus.isReady` and `ClaudeModelInfo.isSubagentModel` are removed; `AgentHUDDesktop` and `AgentHUDOpenApp` build in the Swift 6 language mode.

## 0.4.7 — 2026-09-15

- The application decides and shows island alerts itself: quota alerts and a reminder for each completed turn, for clients whose Live status is on. Baselines start again at every launch, nothing is checked while collection is paused or failing, and turns that finished while Live status was off are not replayed.
- Every subprocess runs under a deadline — the Claude Code engine 40 s, the Codex app-server 30 s, the DeepSeek Harness helper 10 s for a log and 30 s for the balance, `ps` and `lsof` 3 s — and is sent SIGTERM, then SIGKILL after two seconds. A stuck child, or a grandchild holding its pipes, no longer stalls collection.
- Local logs of every client are read through shared ledger-backed file stores. A file missing from its client's listing removes what it recorded, also when its directory is unreadable or gone; a listed file that cannot be read keeps it; after a pass that could not be saved, each source writes back what the ledger lost.
- Host API: `IslandEventTracker` with `Update` and `Crossing`; `DesktopApplication(..., onIslandEvents:)` receives every check with the report and time it used; the public `DesktopApplication.present(_:)` overloads and `IslandAlert.isPreview` are removed; `ClaudeTranscriptParser.title(from:)` becomes `SessionTitle.from(_:)`; `DateParsing` and `ISO8601Fast` move to `Providers/Shared`.

## 0.4.6 — 2026-09-15

- Sessions and token usage from GitHub Copilot CLI, OpenClaw, Hermes Agent, ZCode, CodeBuddy and WorkBuddy, with their logos. GitHub Copilot quota is read only after Settings → Agents → GitHub Copilot → Read quota is confirmed.
- Quota readings belong to the provider account they were read from; readings, history, alert baselines and display settings of different accounts never mix, and Settings → Agents lists the accounts each client has used.
- Usage is collected one source at a time into `usage-ledger.sqlite`: a local poll every 5 s (2 s while indexing) that skips when nothing changed, and an account sweep every 5 minutes. Token charts add up 15-minute totals.
- The unused poll interval setting is removed.
- Host API: `ProviderAccount`, `AccountObservation`, `ClientHome`, `AccountSection` and `UsageStore.accountSections(_:)`; `AgentDescriptor.account` and `windowKey`; `UsageReport.accounts`, `forgottenAccountProviders`, `observation(accountID:)` and `isCurrent(_:)`; `SettingsStore.mergeDiscovered(_:activeQuotaPoolIDs:accounts:)`; `UsageLedger`, `LedgerWriter`, `UsageRefresh`, `AccountRefreshStep`, `UsageBucket` and `CostBucket`; `PollInterval` is removed.
- Known: `scripts/build-app.sh` still stamped `0.4.5` at this tag.

## 0.4.5 — 2026-09-13

- Notch glow styles: besides the blurred band, a halftone dot grid, ASCII characters, shade blocks, Braille and binary digits (`GlowStyle`), with grid pitch, density and spread controls in Settings → Display.
- Motion effects for the grid styles while an agent is running — breathe, flow, scan, ripple, shimmer and boot (`GlowEffect`) — drawn at 24 fps from a display link and eased in and out.
- Running out of quota is its own alert: a window that reaches zero notifies once, even after the earlier at-risk warning.
- Source comments cite `THIRD_PARTY_NOTICES.txt`.
- Host API: `GlowStyle`, `GlowEffect`, `GlowPattern`, `GlowMatrix` and `GlowMotion` in AgentHUDCore; `Settings.glowStyle`, `glowGridPitch`, `glowGridSpread`, `glowGridDensity` and `glowEffect` (an unknown stored value falls back to the blurred glow instead of failing the decode); `QuotaAlertTracker.Update.exhaustedAgentIDs`.

## 0.4.4 — 2026-09-13

- Settings pages supplied by the host appear in the settings sidebar; the window widens to fit the widest page.
- Account refresh is separated from local activity: cached readings stay visible next to a refresh error, liveness follows source observation times, and a slow quota query no longer delays local polling.
- Live status can be switched off per agent in Settings → Agents without affecting collection, history or quota windows.
- The Pi lifecycle observer (`extensions/agent-hud.ts`) is installed automatically when a Pi directory exists; `--install-pi-observer` installs it by hand.
- A completion hook that belongs to another installation is preserved and reported instead of overwritten; `--install-completion-hook` takes ownership explicitly.
- Expired, removed or rejected Kimi / GLM / OpenCode Go credentials retire their quota rows; verified Kimi account identities survive restarts through a hashed local cache.
- Display sliders are debounced; the README shows the animated preview and a statistics screenshot.
- Host API: `DesktopSettingsPage` and `DesktopApplication(options:settings:store:additionalMenuActions:additionalSettingsPages:)`; `DesktopApplication.showSettings(pageID:)`; `UsageProvider.refreshAccountUsage(historyHours:)` with a no-op default; `RetainedUsageProvider` rethrows a failed fetch instead of returning the cached report (`UsageStore.lastError` carries the message while the previous report stays visible); `SessionObservers.configure(executable:)` replaces implicit adapter setup; `CompletionHooks.configure(_:enabled:executable:home:replacingExisting:)`; `PiSessionObserver`; `Settings.liveStatusEnabled(for:)` and `setLiveStatus(for:enabled:)`; `UsageEvent` moved to `Models/` with `TranscriptSession.UsageEvent` kept as an alias; `UsageAnalytics` moved to `Logic/` and `QuotaHistoryStore` to `Store/`; `UsageReport.services` and `activeQuotaPoolIDs`; `LiveSession.observedAt` and `isLive(at:)`; `AgentSettingsGroup.hasLiveStatus`; an `Equatable` overload of `observeChanges`; the `AgentHUDDesktopTests` target.
- Known: `scripts/build-app.sh` still stamped `0.4.3` at this tag.

## 0.4.3 — 2026-09-10

- Kimi turn identifiers taken from loop events match the identifiers reported at turn end, so a finished Kimi turn is recognised as the one that started.

## 0.4.2 — 2026-09-10

- DeepSeek Harness reports explicit running and terminal turn observations (`SessionTurn`) instead of relying on log freshness alone.
- Kimi turns are tracked from the main agent's loop events: `step.begin` starts a turn, `turn.ended` finishes it, and child agents cannot finish the parent.
- New document: session lifecycle coverage.

## 0.4.1 — 2026-09-10

- A quiet DeepSeek turn stays active while a Node process that predates the turn still holds the Harness profile; process inspection reads only executable identity and start time.

## 0.4.0 — 2026-09-10

- Continuous integration: source-boundary check (`scripts/check-source-boundaries.py`, `make check`), unit tests, release build, signature and resource verification.
- Demo mode seeds its preferences with the sample agents.

## 0.3.0 — 2026-09-10

- Native macOS application (`AgentHUDDesktop`, `AgentHUDOpen`): menu bar item, notch glow and hover panel, quota and completion alert views, onboarding, settings (General, Agents, Display), statistics window, snapshot rendering, and the `⌘⌥H` shortcut.
- `Makefile` and `scripts/build-app.sh` produce an ad-hoc signed `build/Agent HUD Open.app` with the bundled logos, `LobeIcons-LICENSE.txt` and `THIRD_PARTY_NOTICES.txt`.
- New document: architecture.
- Host API: `DesktopApplication(options:settings:store:additionalMenuActions:)`, `DesktopMenuAction`, `DesktopLaunchOptions`, `observeChanges`, `SnapshotRunner`.

## 0.2.0 — 2026-09-10

- `AgentHUDCore`: providers for Claude Code, Codex Desktop / CLI, DeepSeek Harness, Antigravity, Cursor, Grok CLI, OpenCode, Kimi, GLM and Pi; usage models, local caches, quota history, alerts, forecasts, chart data, localization and demo data.
- `THIRD_PARTY_NOTICES.txt` and the data-access document.
- Host API: `UsageProvider`, `CombinedUsageProvider`, `RetainedUsageProvider`, `UsageStore`, `SettingsStore`, `UsageReport` and the model types.

## 0.1.0 — 2026-09-10

- `AgentHUDSupport`: `JSONValue` (integer-preserving JSON) and `RecordCoding` (deterministic encoding, millisecond dates, hashed identities).
- Apache-2.0 license and the roadmap.
