# Architecture

The executable creates local preferences, a `CombinedUsageProvider`, and a `UsageStore`, then passes them to `DesktopApplication`.

- `AgentHUDSupport` has no dependency on application services or UI.
- `AgentHUDCore` owns provider-specific data access and normalizes it into `UsageReport`. Local cache files contain reports and indexing metadata.
- `AgentHUDDesktop` observes the supplied store and owns native windows, menu items, and local presentation. Its resource bundle contains all interface assets.
- `AgentHUDOpenApp` selects live or sample data and manages process lifetime.

A host can supply its own `UsageProvider`, observe reports, provide optional menu actions, or present quota and session-completion events. It owns its additional service lifecycle. The shared UI does not initialize account services or data transports.

`UsageProvider.fetchUsage` reads local activity and assembles it with the latest account results. `refreshAccountUsage(historyHours:)` performs the slower quota, balance, and account-wide usage requests. `UsageStore` runs account refreshes independently of its local polling task; providers retain their existing request intervals. Hosts that wrap a provider forward both operations. A one-shot probe can await account refresh before fetching its report.

`RetainedUsageProvider` restores the saved report immediately and merges missing readings after partial failures. A full refresh failure propagates to `UsageStore`, which keeps the previous report and exposes the error. Session `observedAt` records the last activity check; cached running observations age out of the running indicator without inventing an end time or deleting session history.

Hosts opt into adapter setup by calling `SessionObservers.configure(executable:)` with their callback executable. Automatic setup preserves a completion hook owned by another installation and reports the conflict. The standalone `--install-completion-hook` command explicitly transfers that ownership. Creating a `DesktopApplication` does not install adapters.

Shared token events live in `Models/UsageEvent.swift`, usage calculations in `Logic/UsageAnalytics.swift`, and quota history persistence in `Store/QuotaHistoryStore.swift`. The original `TranscriptSession.UsageEvent` spelling remains a type alias for existing library hosts; provider parsers use the shared model.

## Storage and process identity

The standalone application's bundle identifier is `app.agenthud.open`. Its data directory is `~/Library/Application Support/Agent HUD Open`. Demo preferences and snapshot preferences have separate domains. Hosts can set `AgentHUDDataDirectory` in their Info.plist to choose their own cache directory.

SwiftPM resources are located through `AppResources`. App bundles include `AgentHUDOpen_AgentHUDDesktop.bundle` under `Contents/Resources`; command-line SwiftPM builds use the generated module bundle.

## Dependencies

There are no third-party Swift package dependencies. Frameworks and system libraries come from the macOS SDK. Provider clients are discovered on the user's Mac. Install and sign into the clients you want to monitor; unavailable providers are reported independently.
