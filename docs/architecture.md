# Architecture

## Overview

Agent HUD Open is a Swift package with three libraries and one executable. `AgentHUDCore` reads agent activity and account usage on this Mac and normalizes it into a `UsageReport`; `AgentHUDDesktop` presents that report in the menu bar, the notch and the settings and statistics windows; the standalone application wires the two together. A host application can embed the libraries, supply its own provider, add settings pages, and relay the island's alerts. Account services, synchronization and any relay of alerts are outside the package.

## Model

| Module | Responsibility | Depends on |
| --- | --- | --- |
| `AgentHUDSupport` | `JSONValue` (integer-preserving JSON) and `RecordCoding` (deterministic encoding, millisecond dates, hashed identities) | — |
| `AgentHUDCore` | Providers, usage models, calculations, alert decisions, the usage ledger, the settings and usage stores | Support |
| `AgentHUDDesktop` | Menu bar item, notch glow and panel, island alerts, onboarding, settings and statistics windows; its resource bundle holds every logo and notice | Core |
| `AgentHUDOpenApp` (product `AgentHUDOpen`) | Launch options, live or sample data, adapter setup, process lifetime | Desktop, Core |

`CombinedUsageProvider` reads one provider per client, one after another, and merges their reports; `UsageLedger` is the local SQLite store where providers keep parse positions, token events and quota readings, and from which the 15-minute usage and cost totals are read; `RetainedUsageProvider` restores the saved report at start and fills readings a partial refresh could not supply; `UsageCollector` runs the collection pipeline: it waits for sources to signal new data, reads the signalled sources and runs account steps one at a time, and hands each report to `UsageStore`, which publishes the report the desktop observes and tells subscribers what changed. A report carries quota windows, sessions with their token breakdown, turns, completions, 15-minute usage buckets, services, billing and the account inventory (`ProviderAccount`, `AccountObservation`); every quota row belongs to one account.

## Rules

### Host integration

- A host creates a `SettingsStore` and a `UsageStore` around any `UsageProvider`, then a `DesktopApplication`; it owns every additional service and its lifecycle. The shared UI never initializes account services or transports.
- A host supplies `additionalHUDControls` beside the expanded HUD's Stats button. Its factory receives an action that collapses that display's HUD before opening another window; the host owns the controls' state and calls that action before presenting its window. `additionalMenuItems` adds host entries below Settings.
- A host hands its process arguments to `HookEntry.handle(arguments:)` first thing at launch: the clients' hook commands and the adapter commands run there and quit without the interface, and anything else starts the application.
- `UsageAssembly` builds the settings and the store as the standalone application does, over the installed clients with the restart copy on screen, or over the sample data without a ledger, with the host's collection hooks; `UsageProbe.run(settings:)` prints the `--probe` diagnostics.
- `fetchUsage(agents:historyHours:)` assembles local activity with the latest account results; `refreshAccountUsage(historyHours:)` performs the slower quota, balance and account-wide requests at every call and has a no-op default, and the collector alone spaces those calls; `accountRefreshSteps` splits it into steps the store runs between reads, and `watchedDirectories` names the directories whose changes need a read (nil means the provider is read every poll interval). A provider that wraps another forwards all of them.
- `sources` splits a provider into parts read on their own, each with its signals: its directories, its account steps and the checks it asks for. A provider that does not split itself is one source. `fetchUsage(agents:historyHours:sources:)` reads the named sources again and keeps every other source's last result, and `sourceChecks()` names the times at which a source's last result changes with time alone. `accountChecks(since:now:)` names when each source's account steps are next worth running, and `seesLocalWork` is false for a provider whose usage is the account's from every device, which keeps it on the account interval. `CombinedUsageProvider` makes one source per vendor.
- A provider that does not write the ledger reports its periods in `UsageReport.usage`; `CombinedUsageProvider` adds them to the ledger's totals.
- Notices are reported by vendor in `UsageReport.sourceNotices`. A provider says what went wrong with a reading as a `ReadingIssue`, a failed read or an account it could not confirm, at the scope where it read: the vendor's in `readingIssues`, which alone hold back the vendor's alerts, levels, reset credits, retained sessions and Codex window inventory; one account's or plan pool's in `AccountObservation.readingIssue`, which holds back that account's rows; a balance's in `APIBilling.readingIssue`. A notice about local logs or hooks is shown and holds nothing back. A billing pool and its rows answer to their pool's issue alone, never to the vendor's, since each pool is read on its own. Each issue's reason is also written as notice text (`quotaNotices`, `AccountObservation.quotaNotice`, `APIBilling.notice`), and a report without typed issues counts that text as failed reads, or every source notice when it has no `quotaNotices`.
- `UsageReport.status(of:)` gives the status of a window's, an account's or a balance's reading: a window and an account answer to the account's issue, else the vendor's. `assess(_:now:)` adds whether its account is current, its age and its reset. The rows' levels, quota alerts, reset credits, retention and the account headers ask it.
- A failed full refresh keeps the previous report on screen and exposes `UsageStore.lastError`, which the store's view passes to `ReportView` so that every reading of that report counts as failed; a partial failure keeps the missing readings from the saved report.
- `DesktopApplication` decides and presents island alerts itself: one `IslandEventTracker` checks every change of the report, the agent list or the Live status preference while the store is neither paused nor failing, and the island shows the new quota events, added usage resets and completed turns. Every host, the standalone application included, gets the same alerts without extra wiring.
- A host that relays alerts elsewhere passes `onIslandEvents`. It receives every check after the island has presented it, including checks that found nothing, with the report and time the check used; the host maps that update and never runs a second tracker or presents again.
- Completions in an update already honor `Settings.liveStatusEnabled(for:)`; hosts apply the same preference in any other relay or synchronization service they add ([session lifecycle](session-lifecycle.md)).
- A host that receives its clients' prompt and Stop hooks hands the turns they saw to `UsageStore.hookTurns`, by session id, with `isReportedTurn` saying whether a hook turn is the one the report carries for its session; the store's view shows the hook turn's phase wherever the hooks saw more than the logs ([hook turns](session-lifecycle.md#hook-turns)). The standalone application hands in none.

### Collection signals

- A source is read only when it signals new data: a file change under its directories, its account step finishing, one of its checks falling due, or, for a source without directories, its poll interval. Whatever a source does inside, whether it watches files, receives hook callbacks or polls a service, the collector sees only these signals.
- Reads stay serial: signals only mark sources as due, and the collector reads the due sources in one pass; signals that arrive during a pass are read by the next one, and passes start at most every `UsageRefresh.readSpacing`.
- Time-based changes of activity are checks, not polls: a session whose source never said what its turn is doing is read again when it reaches `SessionPhase.Limits.quiet` without an observation, and a running turn when it stops counting as current work and again at `SessionPhase.Limits.abandoned`, where silence means its client is gone.
- File events name real paths; the collector compares them with each directory as given and as `realpath` resolves it.
- A source's account steps run when its own work, or one of its windows resetting, makes a new reading worth taking; opening the panel, the menu or the statistics window reads every account, and no source repeats a request within `UsageRefresh.accountRequestSpacing`.
- A settings change that needs a read wakes the collector at once, through `SettingsStore.onChange`, which a running `UsageStore` sets: a changed agent list reads every source, and GitHub Copilot quota reading switched on or off reads Copilot's quota at once, within the request spacing too, and every other account the spacing allows. The pass it wakes is serial like any other. `CombinedUsageProvider.standard(settings:ledger:persistent:)` takes the same `SettingsStore`, and the Copilot provider asks it for the consent whenever a reading is due.
- Every source is read again every `UsageRefresh.accountInterval`, which catches a file event the watch missed, and a refresh reads every source once.
- `UsageStore.observeChanges(_:)` calls a handler with `UsageChanges` for every newly displayed report: usage totals, readings, the ids of changed sessions, turns, new completions and the inventory. Publishers subscribe instead of comparing reports.

### Collection hooks

- A host passes `UsageCollectionHooks` to `UsageStore`; every hook is optional, and without them the store displays the provider's report and loads `UsageStore.historyHours` hours, the seven-day range plus the partial hour.
- `historyHours` is asked before every local poll and account step, so a host can widen the window providers read.
- `publish` receives each report the provider returned and `merge` turns it into the displayed report, for example to add other data. Both are awaited inside the pass, after the provider fetch and before the report is displayed; no provider reads and nothing writes the ledger meanwhile, so a hook that reads the ledger stays serial with collection.
- The merged report keeps the provider's discovered agents, accounts, active quota pools, completions and turns: the agent list and island events read them from `UsageStore.report`.
- `UsageStore.remerge()` runs only `merge` again on the provider's last report, for data the host merges that changed since the pass. It never overlaps a local poll: a poll in progress merges for it, or merges again when its own merge had already started. A report installed with `replace(report:)` is not merged over.

### Session observers and hooks

- `SessionObservers.configure(executable:enabled:)`, called after creating the store and before `start()` with `Settings.clientHooks`, installs the Pi and OpenCode observers when those clients' directories exist, the Antigravity, Cursor, GitHub Copilot CLI, CodeBuddy and Qwen Code stop hooks, Claude Code's notification hook and each detected client's approval hook when those clients are installed, or with `enabled` false removes Agent HUD's handlers from them. Creating a `DesktopApplication` installs nothing; a change of `clientHooks` while it runs applies at once with the main bundle's executable, and while `clientHooks` is on, an observer directory that appears after launch (`SessionObservers.observedClients()`) gets its observer at the next report (`SessionObservers.installObservers()`).
- Every Agent HUD handler belongs to the copy that runs, since only one runs at a time: `configure` points each one at the executable it is given, whichever copy wrote it, and adds none from under App Translocation or `/Volumes` ([completion hooks](session-lifecycle.md#completion-hooks)).
- `HookSettings.write(_:to:)` writes every client settings file the hooks change, and a host's own hooks can use it too: through a symbolic link to the file it leads to, keeping the file's permissions.
- `HookInstaller` reads one hook's settings file, up to 16 MB, and points Agent HUD's handlers in it at the running copy or takes them out, the hook's own edit given; `HookCommand` makes and recognizes a handler's command. A host's own hooks use both.
- `UnixSocketListener` is the application's end of the socket a hook process reaches through `UnixSocket`: only the user can reach it, a request is one message the sender ends by closing its side, and one process serves a path.

### Storage

- The standalone bundle identifier is `app.agenthud.open`; preferences live in its UserDefaults domain, with separate domains for demo and snapshot runs.
- The usage ledger, the restart copy of the report (including account labels), hashed Kimi identities and completion records live in the data directory ([storage](providers.md#storage)); token events keep 31 days, quota readings 30 days and API balance readings one day. No file contains credentials, and the only conversation text in them is a session's title, which can be the first line of its first prompt, 60 characters at most.
- SwiftPM resources are located through `AppResources`; the app bundle carries `AgentHUDOpen_AgentHUDDesktop.bundle` under `Contents/Resources`.

### Design invariants

- Integer precision: whole numbers decode as `Int64` before `Double`; token counts are never rounded or interpolated.
- Deterministic identities: record ids are SHA-256 hashes of length-prefixed components, identical on every machine.
- Millisecond dates: persisted dates round-trip through milliseconds since 1970.
- Tests need no credentials or network; the only probe that touches installed clients is opt-in.
- Isolated sources: a missing, signed-out or failing client never hides another.
- One copy at a time: every application built on these libraries shares `InstanceLock`, whatever its data directory or bundle identifier. A host calls `SingleInstance.claim()` at launch, after its probes and before it opens preferences, the ledger or a client's settings, and quits when it returns false.
- No invented lifecycle: inactivity is never a completion, and a passed reset deadline is not a confirmed reset.
- Observed rows only: rows, groups and first-launch entries come from what providers report or find on the Mac, never from a built-in list of placeholders.
- Names apart from ids: vendor ids key settings, the ledger, accounts and sync records and never change; `VendorCatalog` holds the names shown, and a value it does not name is shown as written, never filed under another.
- Resources and notices: the root `THIRD_PARTY_NOTICES.txt` and the bundled copy stay byte-identical, and source comments cite the file.
- Source boundaries: `make check` rejects signing material, private service directories and imports, secret-shaped strings, external package dependencies, missing ignore rules and a bundled notices file that differs from the root one.

### Versioning

- A release is a tag `vX.Y.Z` on `main`; the bundle's `CFBundleShortVersionString` equals `X.Y.Z` at that tag, `CFBundleVersion` stays `1`, and the [changelog](../CHANGELOG.md) has a matching entry that lists host-visible API changes. Hosts pin the package by tag or commit.
- There are no third-party Swift package dependencies; frameworks come from the macOS SDK.
- Continuous integration runs the boundary check, `swift test`, a release build, `codesign --verify --deep --strict`, a check that the resource bundle contains the logos, a check that no provisioning profile was embedded and one that the executable names the SDK it was built against, not its deployment target, within 30 minutes. It publishes no binaries.

## Interfaces and configuration

| Item | Source | Meaning |
| --- | --- | --- |
| `UsageStore(provider:settings:accessAllowed:hooks:)` | AgentHUDCore | Any `UsageProvider`, the settings store, an access closure (false pauses collection) and `UsageCollectionHooks` |
| `UsageCollectionHooks(historyHours:publish:merge:)` | AgentHUDCore | `@MainActor () -> Int`; `@MainActor (UsageReport) async -> Void`; `@MainActor (UsageReport) async -> UsageReport` |
| `start()`, `stop()`, `refresh()`, `remerge()`, `replace(report:)`, `pause(for:)`, `resume()` | `UsageStore` | Collection lifecycle; `refresh` reads every source now unless a pass is running; `replace` installs a report without the provider |
| `hookTurns` | `UsageStore` | `[String: SessionPhase.HookTurn]` by session id: the turns a host's prompt and Stop hooks saw |
| `observeChanges(_:)` → `UsageChangeObservation` | `UsageStore` | `@MainActor (UsageChanges) -> Void` for each newly displayed report that changed something; releasing or cancelling the observation ends it |
| `UsageSource(name:directories:accountSteps:)` | AgentHUDCore | A provider's independently read part and its signals; nil `directories` is read every `UsageRefresh.pollInterval` |
| `DesktopApplication(options:settings:store:additionalSettingsPages:onIslandEvents:)` | AgentHUDDesktop | Parsed `DesktopLaunchOptions`, the two stores, `[DesktopSettingsPage]` and an optional `(IslandEventTracker.Update, UsageReport, Date) -> Void` relay hook |
| `start()`, `stop()`, `showSettings(pageID:)`, `showStats()`, `showOnboarding()`, `toggleGlow()` | `DesktopApplication` | Host entry points. `showSettings` selects a page by id (built-in `general`, `sources`, `display`; an unknown id keeps the current page) |
| `IslandEventTracker.Update` | AgentHUDCore | `completions` (new, Live status on, oldest first), `quotaAlerts` (warnings, exhaustion, resets), `exhaustedWindows` and `criticalWindows` (threshold crossings with the reading that crossed; reaching zero supersedes critical in the same reading), `resetCreditGrants` (accounts whose Codex reset-credit count rose, with the added credits when the provider lists them) |
| `DesktopSettingsPage` | AgentHUDDesktop | `id`; `title` closure (follows language changes, also the page heading); `subtitle`; `symbol` and `color` for the sidebar icon; `preferredContentWidth` in points (built-in pages use 640); `@ViewBuilder` `content` |
| Settings window | AgentHUDDesktop | 760 × 720 points, minimum 680 × 560, sidebar 212; the initial width grows to fit the widest host page |
| `SingleInstance.claim(at:)` | AgentHUDDesktop | Takes the shared lock and keeps it for the process lifetime; false after an alert naming the copy that holds it. A lock that cannot be created returns true |
| `InstanceLock.claim(at:executable:)` → `Claim` | AgentHUDCore | `acquired` (the lock lasts as long as the value), `held(by:)` (the application that runs, its bundle when it has one) or `unavailable`; `InstanceLock.sharedURL` is `~/Library/Caches/app.agenthud/instance.lock` |
| `SessionObservers.configure(executable:)` | AgentHUDCore | Adapter setup with the executable that handles hook callbacks |
| `HookSettings.write(_:to:)` | AgentHUDCore | Writes a client's settings object as sorted, pretty-printed JSON through its symbolic links, keeping the file's permissions |
| `HookInstaller(configuration:arguments:)` | AgentHUDCore | `read()`, `owns(_:)` and `configure(enabled:executable:updating:willWrite:)` for one hook of one client |
| `HookCommand` | AgentHUDCore | `make(executable:arguments:)`, `runs(_:arguments:)`, `isTransient(_:)` and `checkInstall(executable:)` |
| `HookEntry.handle(arguments:)` | AgentHUDCore | Runs the hook or adapter command the process arguments name and returns its exit status; nil starts the application |
| `UnixSocket`, `UnixSocketListener(path:requestLimit:category:onRequest:)` | AgentHUDCore | The hook process's `connect(to:)`, `send(_:_:)` and `readToEnd(_:limit:)`; the application's `start()` and `stop()`, with each request handed over on the main actor |
| `ChildProcess` | AgentHUDCore | A child whose pipes are drained up to their caps and whose reads end when it exits: `run(_:_:environment:timeout:stdoutLimit:)`, or `write(_:)`, `line(before:)`, `waitForExit(before:)` and `stop()` |
| `UsageAssembly.settings(defaults:defaultAgents:language:)`, `store(settings:ledger:hooks:)` | AgentHUDCore | The settings and the store as the standalone application builds them; a nil ledger is the demo, with the demo's turn calls in `UsageStore.sampleTurnCalls` |
| `UsageProbe.run(settings:)` | AgentHUDCore | Prints the `--probe` diagnostics and returns the exit status |
| `AgentHUDDataDirectory` | Host `Info.plist` | Name of the data directory under `~/Library/Application Support`; default `Agent HUD Open` |
| Launch switches and probes | Standalone executable | [Command line](command-line.md) |

## Code map

| Concept | Code |
| --- | --- |
| JSON values, record coding | `Sources/AgentHUDSupport/JSONValue.swift`, `RecordCoding.swift` |
| Provider protocol, combination, retention, observers | `Sources/AgentHUDCore/Providers/UsageProvider.swift`, `CombinedUsageProvider.swift`, `RetainedUsageProvider.swift`; `Sources/AgentHUDCore/Hooks/SessionObservers.swift` |
| Per-client providers, and the toolbox they share: JSON, files, HTTP, SQLite, log stores | `Sources/AgentHUDCore/Providers/<Client>/`; `Sources/AgentHUDCore/Providers/Kit/`, `Providers/Kit/Logs/` |
| Models: usage, sessions, accounts, agents, catalog, settings; calculations: quota, alerts, what the Mac shows | `Sources/AgentHUDCore/Models/<Area>/`; `Sources/AgentHUDCore/Logic/`, `Logic/Quota/`, `Logic/Alerts/` |
| Durations, token counts, money, dates and languages as written | `Sources/AgentHUDCore/Formatting/` |
| Vendor names, client and window names, app bundle IDs | `Sources/AgentHUDCore/Models/Catalog/VendorCatalog.swift`, `Formatting/WindowNames.swift` |
| Collection pipeline, signals and hooks; change sets | `Sources/AgentHUDCore/Store/UsageCollector.swift`, `System/FileChangeMonitor.swift`, `Models/Usage/UsageChanges.swift` |
| Stores and data directory; the statistics window's selections and what they chart | `Sources/AgentHUDCore/Store/UsageStore.swift`, `SettingsStore.swift`, `UsageStore+Stats.swift`, `Ledger/QuotaHistoryStore.swift`, `System/AppSupport.swift`; the usage ledger in `Sources/AgentHUDCore/Ledger/` |
| Application object, launch options, host pages | `Sources/AgentHUDDesktop/App/DesktopApplication.swift`, `LaunchOptions.swift`, `Settings/DesktopSettingsPage.swift` |
| One copy at a time | `Sources/AgentHUDCore/System/InstanceLock.swift`, `Sources/AgentHUDDesktop/App/SingleInstance.swift` |
| Island alerts: decision and presentation | `Sources/AgentHUDCore/Logic/Alerts/IslandEvents.swift`, `QuotaAlerts.swift`; `Sources/AgentHUDDesktop/Notch/Island/IslandController.swift`, `Notch/Alerts/IslandAlert.swift` |
| Standalone entry, launch assembly and probe | `Sources/AgentHUDOpenApp/main.swift`; `Sources/AgentHUDCore/Store/UsageAssembly.swift`, `UsageProbe.swift` |
| Hook and adapter commands, hook installation and records, the socket to hook processes, child processes | `Sources/AgentHUDCore/Hooks/HookEntry.swift`, `HookInstaller.swift`, `HookInbox.swift`, `HookCommand.swift`; `Sources/AgentHUDCore/System/UnixSocket.swift`, `UnixSocketListener.swift`, `ChildProcess.swift` |
| Build, boundary check, CI | `scripts/build-app.sh`, `scripts/check-source-boundaries.py`, `.github/workflows/ci.yml` |

## Related

[providers.md](providers.md) per-client data sources · [usage-semantics.md](usage-semantics.md) counting and alert rules · [session-lifecycle.md](session-lifecycle.md) turn evidence and hooks · [data-access.md](data-access.md) boundaries · [../CHANGELOG.md](../CHANGELOG.md) host-visible API changes
