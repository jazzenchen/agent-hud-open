# c02d — OpenAgents (Kimi, OpenCode/Go, GLM, Pi) and Additional (sub-audit of c02)

Counts (60 rows): same 16 · deliberate 1 · better 0 · missing 9 · differs 33 · unclear 1.

## Missing / differs / unclear
| Open | Behavior | Next | Status | What / when / user sees |
|---|---|---|---|---|
| ProviderHTTP.swift:9 | Quota timeout 12 s | agents/src/coding_plans/mod.rs:112 15 s | differs | Slow error 3 s later |
| OpenAgentCredentials.swift:142-149 | KIMI_CODE_BASE_URL matched by service (`coding` or `coding/v1`) | exact `…/coding/v1` | differs | `…/coding` → Kimi OAuth slots skipped, no Kimi plan quota |
| OpenAgentCredentials.swift:60-71,200-211 | GLM pool key: provider, realm, product, hash(token) | account id includes base URL | differs | Same GLM key in Claude and OpenCode/Pi → plan shown twice, requested twice |
| OpenAgentCredentials.swift:14-16; UsageProvider.swift:238-247 | WorkBuddy-only GLM plan shown in WorkBuddy group "GLM · <pool>" | GLM reader runs only when GLM detected | missing | No rows for a WorkBuddy-only plan |
| OpenAgentUsageProvider.swift:100-102 | Every discovered plan credential read regardless of client | Kimi read only when the kimi provider is detected | missing | Kimi plan via Claude/OpenCode/Pi without Kimi CLI → no quota |
| OpenAgentUsageProvider.swift:56-59,104-129,269-270 | Kimi /me identity cached; failure notice "Account identity unconfirmed" | /me every read, no cache, silent fallback id | differs | Account id changes, rows re-keyed/duplicated |
| OpenAgentUsageProvider.swift:130-153 | Aliases tried until one succeeds | every entry read, failed alias counted | differs | Extra request; false "could not be read" |
| OpenAgentUsageProvider.swift:137-153,243-261 | Per-account failure: keep last reading + readFailed; all 401/403 → retire | per provider | differs | Failed account vanishes; stale rows kept under English notice; revoked keys not retired |
| OpenAgentUsageProvider.swift:195-196,233-234 | No credential → no notice | notice "No usable coding-plan credential…" | differs | Extra notice in Settings |
| OpenAgentQuota.swift:41-53,156-195 | Full names "<period> · <plan>", periods ±5 % | no plan suffix, exact periods | differs | "5-hour quota" vs "5-hour quota · <plan>"; "30-day limit" vs "Monthly limit" |
| OpenAgentQuota.swift:134-152 | Short labels in Core | TS regex | fixed | "5-hour quota" instead of "5h" **Fixed 2026-10-20** (window names): Kimi, GLM and OpenCode Go windows carry Open's full and short names (`kimi_name`, `glm_name`, periods within 5 %), and GLM's token and credit windows of one length read "Tokens 5h" and "Credits 5h". |
| BillingPool.swift:29-34 | Account label "<realm> · PLAN · Account <6 hex>" | plan name only | differs | CN vs international pools indistinguishable |
| OpenAgentCredentials.swift:64 | OpenCode Go is its own group | under vendor opencode | differs | Go windows mixed into OpenCode group |
| OpenAgentQuota.swift:71-76,127-128 | used/capacity rules | nativeUsage on Kimi; GLM capacity dropped | differs | Extra/missing consumed figures |
| OpenAgentQuota.swift:146 | Duplicate window ids are a format error | first kept silently | differs | No error |
| OpenAgentUsageProvider.swift:176-208,283 | Quota windows linked to models (consumerSource) | core/src/database/quota_link.rs | fixed | Tokens/hour, cost, reply rate unavailable for plans, Antigravity, Copilot, Cursor, Devin, Grok Bot **Fixed 2026-11-05**: every window whose spending is logged is linked, by its client (`consumerSource`), the `#route` its calls are logged under (`consumerRoutes`, `client#route` for one client's alone) and its models' families. Kimi, GLM and OpenCode Go plans count their own provider ids and, when Claude Code sends the plan key, Claude Code's calls to the vendor's models; MiniMax the provider ids its plan key is kept under; Codex Pi's ChatGPT calls. Not linked: Ark (plan and pay-as-you-go share an address), Trae (no token logs), spending pools that take only overflow (Cursor on-demand, Grok extra, WorkBuddy rewards). |
| AgentAPIServiceDiscovery.swift:5-91; PriceRegions | API/plan service discovery with China/International region | agents/src/coding_plans/mod.rs `plan_service` (plans); none for API keys | missing | Plan services with their platform **fixed 2026-10-20**, so China-plan usage is priced in CNY. Still missing: API-key services (OpenCode, Pi, Claude settings), and Settings lacks per-client plans/APIs. |
| OpenAgentUsageProvider.swift:66-67 | Import earlier quota history for plan windows | only rows with legacyWindowId | differs | Burn/outlook start empty |
| CombinedUsageProvider.swift:66-74 | Account step order | different order (both serial) | differs | First readings in different order |
| CombinedUsageProvider.swift:261-267 | Running OpenCode/Kimi/Pi turn → 150 s cadence for the whole step | keyed by reader id; glm always 300 s | differs | Plan quota refreshes up to 2× slower |
| OpenAgentSessions.swift:297-317 | OpenCode SQLite from max(7 d) day start, 5000-row statements, one read | whole table, 1000 rows/pass, clears on new generation | differs | Session/total tokens dip during re-read while OpenCode writes |
| OpenAgentLocalStore.swift:24-26 | Legacy JSON up to 64 MiB | 2 MiB | differs | Usage lost with schema notice |
| OpenAgentLocalStore.swift:30-37,87 | JSON and SQLite copies merge per message | JSON dropped for sessions in DB | differs | JSON-only messages not counted |
| OpenAgentSessions.swift:250-269 | Untitled OpenCode → workspace folder or "OpenCode" | raw id | fixed | "opencode:ses_…" titles **Fixed 2026-10-20** (session titles). |
| OpenAgentSessions.swift:163-176; LocalStore.swift:43-72 | Kimi title from state.json; workspace from workspaces.json | none | fixed | "kimi:<folder>:main" titles, no project **Fixed 2026-10-20** (session titles). |
| OpenAgentSessions.swift:110-151 | Pi title first line of first prompt; /name clearing; "Pi" | whole prompt ≤100, keeps old name, id | fixed | Different titles **Fixed 2026-10-20** (session titles). |
| OpenAgentLocalStore.swift:73-86 | Observer title fills placeholder | never | differs | Id/placeholder titles |
| OpenAgentSessions.swift:173-176 | Children listed flat | nested under parent | deliberate | Fewer top-level rows |
| OpenAgentSessions.swift:122-125 | Pi model_change sets provider | model only | differs | Data only |
| OpenAgentSessions.swift:235-242 | Kimi StatusUpdate replace when total ≥ old, first time kept | per-field max, latest time | differs | Data only |
| OpenAgentSessions.swift:94-134 | Corrupt line fails file with notice; caps | skip line; >1 MiB lines skipped; no caps | differs | Partial usage; large Pi lines lost |
| UsageProvider.swift:233-291 | Notices per source and plan group (zh/en) | one global English message | differs | No per-client notice |
| OpenAgentSessions.swift:9-12 | Source descriptions zh/en | English technical notes; GLM "not migrated" | differs | Settings text |
| OpenAgentSessions.swift:13-18 | Install detection by roots/install paths | also config folders, PATH binaries, desktop apps | differs | Sources detected (and readers started) where Open says not detected |
| AdditionalUsageProvider.swift:61,87-93,139-147 | Grok CLI quota; Bot cache trigger; Grok CLI navigation | none | missing | See c02a |
| AdditionalUsageProvider.swift:120,173,259 | Copilot consent withdrawal forgets history | kept | differs | Old burn history returns |
| ProviderData.swift:41-46 | 8-day reader windows | whole database | differs | Data only |
| AdditionalUsageProvider.swift:134-260 | WorkBuddy ceilings declare account inventory | no accountId, no retirement | differs | Ceiling rows not attached to an account |
| AdditionalLocalStore.swift:27-41 | Same session id in several files merges by id, conflict notice | canonical contribution | unclear | Needs a two-copy fixture |
| OpenAgentDiagnostics.swift | `--probe-open-agents` | none | missing | Developer diagnostic only |

Same (16): credential sources/order, official endpoints, endpoints/headers, Kimi/Go/GLM parsing, serial account reads, session roots, ids/identity, token categories, OpenCode SQLite schema handling, archived OpenCode, Pi heartbeat, retained readings, layouts, hook completion. Deliberate: own ledger database.

Behaviors summarized above, counted for the ledger totals:

| Behaviors | Status |
| --- | --- |
| Matching behaviors listed under "Same" | same ×16 |
| Next's own ledger database | deliberate ×1 |

## Tests
All portable to fixtures (injected fetches, temp files, SQLite) except the last 3 WorkBuddyGLMPlan tests (SettingsStore), AntigravityCLI run tests, and the env-gated real-machine probe.

## Architecture
- Short window names and the "Untitled session" fallback in TS/UI. Short window names fixed 2026-10-20.
- Serial-rule gap: request_snapshot with due reminders → read_reminded → scan outside the gate (fixed for Settings paths on 10-20 by routing through the collector worker; set_provider path still open).
- Each plan reader runs the full credential discover() separately (three reads per cycle).
- ceiling_quotas writes workbuddy-ceilings.json during snapshot render.
- Next adds HUD-owned API credential slots with no Open counterpart.
