# Architecture

The executable creates local preferences, a `CombinedUsageProvider`, and a `UsageStore`, then passes them to `DesktopApplication`.

- `AgentHUDSupport` has no dependency on application services or UI.
- `AgentHUDCore` owns provider-specific data access and normalizes it into `UsageReport`. Local cache files contain reports and indexing metadata.
- `AgentHUDDesktop` observes the supplied store and owns native windows, menu items, and local presentation. Its resource bundle contains all interface assets.
- `AgentHUDOpenApp` selects live or sample data and manages process lifetime.

A host can supply its own `UsageProvider`, observe reports, provide optional menu actions, or present quota and session-completion events. It owns its additional service lifecycle. The shared UI does not initialize account services or data transports.

## Storage and process identity

The standalone application's bundle identifier is `app.agenthud.open`. Its data directory is `~/Library/Application Support/Agent HUD Open`. Demo preferences and snapshot preferences have separate domains. Hosts can set `AgentHUDDataDirectory` in their Info.plist to choose their own cache directory.

SwiftPM resources are located through `AppResources`. App bundles include `AgentHUDOpen_AgentHUDDesktop.bundle` under `Contents/Resources`; command-line SwiftPM builds use the generated module bundle.

## Dependencies

There are no third-party Swift package dependencies. Frameworks and system libraries come from the macOS SDK. Provider clients are discovered on the user's Mac. Install and sign into the clients you want to monitor; unavailable providers are reported independently.
