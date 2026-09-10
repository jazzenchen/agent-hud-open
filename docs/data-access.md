# Data access

Agent HUD Open reads agent activity and usage metadata on your Mac. It has no Agent HUD account, cloud synchronization, or push service.

## Providers

| Client | Local data | Quota or balance queries |
| --- | --- | --- |
| Claude Code | Session records and account profile | Installed Claude engine usage interface |
| Codex Desktop / CLI | Session records, including `CODEX_HOME` | Installed Codex app-server account rate limits |
| DeepSeek Harness | Session records and profile-owning Node process metadata, including `DSH_HOME` | Official DeepSeek balance endpoint with the configured Harness API key |
| Antigravity | Local application process and conversation metadata | Running application's local language server |
| Cursor | Local application database and session metadata | Official Cursor usage endpoints with the installed client's session token |
| Grok CLI | Local session records and credential file | Official Grok CLI billing endpoint |
| OpenCode, Kimi, GLM, Pi | Local JSON/SQLite session records and supported provider configuration | Official Kimi, GLM, and OpenCode Go quota endpoints where configured |

Some providers read agent API keys or tokens from their own configuration, environment variables, or local credential files. Credentials are used only for the corresponding provider's usage request. They are not included in reports or persisted to the HUD's caches. Custom endpoints are not assumed to share official billing accounts, and executable key resolvers are not run.

Claude and Codex quota queries use the installed clients' existing sign-in. Codex `auth.json` is not read directly. The app does not request model responses or consume usage-reset credits.

DeepSeek's open turns remain active during quiet tools or questions while a Node process holds the same Harness home's profile and predates the turn. Process inspection reads executable identity and start time, not profile contents or browser credentials. Without that evidence, activity falls back to recent log updates. A process started after a turn does not keep that old turn active.

## Local storage

Preferences use the application's UserDefaults domain. Cached reports, quota observations, and session indexes are stored in `~/Library/Application Support/Agent HUD Open`. Local metadata can include session titles and workspace paths. Raw conversation bodies and authentication secrets are not copied into these caches.

Quota requests are throttled independently from local activity polling. Missing or signed-out clients do not prevent other sources from reporting. A failed refresh retains the last successful readings; unavailable quotas are not inferred from token counts.

Optional completion hooks write a small local event record to identify finished sessions. They do not send notifications or upload data.

## Building

Building the application requires no developer account, product credential, provisioning profile, or signing certificate. Local builds use ad-hoc signing. Provider protocol references and licenses are included in [third-party notices](../THIRD_PARTY_NOTICES.txt).
