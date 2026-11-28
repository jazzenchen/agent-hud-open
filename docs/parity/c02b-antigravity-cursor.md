# c02b — Antigravity providers and CursorClient (sub-audit of c02)

Open: 7 files, 1,295 lines. Counts (51 rows): same 21 · deliberate 1 · better 0 · missing 7 · differs 22 · unclear 0.

## Check of the earlier audit
- "Cursor request-level billing sessions" gap is refuted as written: fetch_events, reconcile_pages, parse_events, resolve_local_names (agents/src/cursor/events.rs and agents/src/cursor/sessions.rs) ingested by ingest_account_sessions.
- Real Cursor gaps: accountWide flag, failure notice, sign-out clearing, coupling to the quota read, local-midnight window, BLOB values, 1 MiB cap.
- Missing Antigravity quota summary (RetrieveUserQuotaSummary) is not listed as a gap anywhere; docs/accounts.md:17 describes GetUserStatus only.

## Missing / differs
| Open | Behavior | Next | Status | What / when / user sees |
|---|---|---|---|---|
| Antigravity/AntigravityClient.swift:46-56,96-131 | Quota from RetrieveUserQuotaSummary (groups, buckets, 5h/weekly durations) | clients/antigravity/service.rs; agents/src/antigravity/buckets.rs | fixed | **Fixed 2026-11-27**: `RetrieveUserQuotaSummary` with `GetUserStatus` fallback, unified `Carbon` / `Argon` / `Gemini` / `3rd-party` 5h (`18000s`) and weekly (`604800s`) bucket classification, untouched rolling-reset suppression, and idle `antigravityHomes` background `/usage` polling. |
| AntigravityClient.swift:61-94 | supersededLegacyWindowIDs, summaryNames/compactName | agents/src/antigravity/buckets.rs; quotas.rs | fixed | **Fixed 2026-11-27**: summary buckets supersede legacy status buckets and emit exact family/period short labels (`Carbon 5h`, `Argon 7d`, `3rd-party 5h`, etc.). |
| AntigravityCLI.swift:10-44 | Fallback `agy -p /usage --output-format json` | agents/src/antigravity/cli.rs | fixed | **Fixed 2026-10-22**: the running CLI's executable supplies its weekly and five-hour groups when no identified account is available. The response names no account, so its identity remains local-home. |
| AntigravityService.swift:88-108 | Candidate priority CLI=1, Antigravity IDE=2 | clients/antigravity/service.rs:read_accounts | better | **2026-10-22**: every readable running service contributes its own account. Email and team identity deduplicate the same sign-in across apps; a failed service does not discard other accounts. |
| n/a | Identified multiple CLI accounts from official status-line reports | agents/src/antigravity/statusline.rs; platform status-line callback/installer | better | **2026-10-23**: connected CLI settings report email, plan and exact quota windows; different emails remain separate, duplicate sign-ins merge, prior observations stay visibly stale, and existing user status-line commands are preserved. |
| AntigravityService.swift:27-30,55-68 | Approval discovery remembered 10 s; all listeners of all processes | clients/antigravity/service.rs:176-257 | differs | A second service's approvals never reach the HUD while the first answers |
| AntigravityClient.swift:11-17,43 | Notices for not running / no readable quota | src/crates/agents/src/antigravity/account.rs:109-114,132-145 | differs | Failed reading marks rows stale; different English-only text The notices are bilingual since 2026-10-20; a failed reading still marks rows stale. |
| AntigravityClient.swift:27-31 | Empty legacy fallback is not a reading | src/crates/agents/src/antigravity/account.rs:109-114 | differs | "connected, no percentage" instead of summary windows |
| AntigravityClient.swift:133-159; CursorClient.swift:87-98 | Short labels set in Core | ui hud/model.ts:188-195 regex | fixed | "Claude and GPT" vs "3rd-party"; "Kimi K3" vs "K3"; Cursor "Included usage" vs "Included" etc. **Fixed 2026-10-20** (window names): "3rd-party", the model's word ("K3"), and Cursor's "Included", "First-party", "Third-party", "Spend limit", "Pooled", "On-demand" come from Core. The summary schema's compact names wait for that schema (not read by Next). |
| ProviderData.swift:112-123; ids | legacyWindowId mapping for Cursor/Antigravity rows | not set (only Claude/Codex set it) | differs | First Next run: these rows start with no quota history; legacy visibility/order don't map |
| AntigravityClient.swift:141-144 | Unresolved account key home "" | accounts/mod.rs:92-97 home:<path> | differs | Data only |
| AntigravitySessions.swift:36-84,108 | Title: escapes decoded to bytes then UTF-8; first line ≤60 with …; empty → "Client · stem" | agents/src/../core/src/clients/antigravity.rs:38-104,138-151 | fixed | Non-ASCII titles garbled ("ä½" instead of "你"); long/multi-line titles **Fixed 2026-10-20** (session titles). |
| AntigravitySessions.swift:89-134 | Notice for excluded records | silent skips | missing | Tokens silently excluded |
| AntigravitySessions.swift:90-96 | No blob size cap | skips >1 MiB | differs | Large generation tokens missing silently |
| AntigravitySessions.swift:106-109 | Client "Antigravity CLI" vs "Antigravity" | source only | differs | CLI label gone from rows |
| AntigravitySessions.swift:124-131 | Dedup keeps first, conflict notice | lowest idx only | differs | Rare dropped tokens |
| AntigravitySessions.swift:108-109 | Reader sessions have no navigation target | ingestion.rs:199-203; checkpoints.rs:161-164 | differs | Click tries to open in app; fails for agy CLI |
| AntigravityConversation.swift:9-186 | Prompts and replies from steps | conversation.rs:345-346 refuses SQLite | missing | Session detail: "Message paging is not available" |
| CursorClient.swift:28-42 | Token/email as TEXT or UTF-16LE BLOB | agents/src/cursor/account.rs:76-95 String only | differs | BLOB storage → Cursor quota and sessions fail entirely |
| CursorClient.swift:38,46,53 | One sign-in message | three English messages | differs | Wording |
| QuotaMath.swift:25-27 | Remaining clamped 0…100 | HUD meter label unclamped (model.ts:198-201) | differs | "112%" on HUD |
| CursorClient.swift:110-112 | Window from local midnight, max(168,169) h | UTC midnight − 7 d | differs | Up to the UTC offset of the oldest day never fetched |
| CursorClient.swift:113-120 | Sign-out / account switch clears sessions | kept | differs | Previous account's sessions stay listed |
| CursorClient.swift:132-139 | Dashboard failure notice | unwrap_or_default | missing | Data silently stops updating |
| AdditionalUsageProvider.swift:113-116 | Sessions refresh independently of quota | `?` before sessions | differs | Quota failure also stops session refresh |
| ProviderHTTP.swift:36 | 16 MiB response cap | accounts/http.rs 1 MiB | differs | Heavy users' event pages fail silently |
| CursorClient.swift:203-237 | Title clip 60 with … | trim, 100, no … | fixed | Minor **Fixed 2026-10-20** (session titles). |
| CursorClient.swift:261 | accountWide: true | none | missing | Other devices' Cursor sessions listed like local ones, log path /account/cursor |
| all L10n | zh variants | English only | fixed | English notices/titles **Fixed 2026-10-20** (bilingual text): Antigravity and Cursor notices and the Grok Bot sub-agent title are in both languages. |
| n/a | Windows Antigravity discovery | os/windows | deliberate | New Windows adapter |

Same (21): endpoint/headers, ps gating, flag/port parsing, command runner, account hash, legacy userStatus parse, session roots, db selection, model order and token math, matchedTimestamp, protobuf reader and field numbers, Cursor db path, JWT/cookie, usage-summary fetch and parse, events paging and reconcile, event parsing, automations, Grok Bot spend merge.

Behaviors summarized above, counted for the ledger totals:

| Behaviors | Status |
| --- | --- |
| Matching behaviors listed under "Same" | same ×21 |

## Tests
Mostly portable (JSON/SQLite fixtures, recorded HTTP pairs); `AntigravityCLI.executable` and actor/ledger-bound integration tests are not.

The Rust account tests cover distinct accounts, duplicate sign-ins, anonymous
services, partial failure, CLI fallback, quota groups, and unfinished CLI output.
Status-line fixtures also cover sanitized persistence, missing/invalid quota,
sample age, reset cycles, account switches and CLI/native identity deduplication.

## CLI account selection investigation (2026-10-22)

The installed `agy 1.3.5` exposes no profile, account, or home selector in
`--help`; `agy help auth` returns `unknown subcommand`. Its binary does not
contain `GEMINI_CLI_HOME`. `ANTIGRAVITY_APP_DATA_DIR` is emitted into a child
tool's environment, rather than read as the CLI's account selector. The
[official authentication guide](https://www.antigravity.google/docs/cli/install/)
describes the operating system's keyring, while the
[command reference](https://www.antigravity.google/docs/cli/reference/) documents
`/usage` and `/logout` but no multiple-profile contract.

Consequently, arbitrary inactive CLI homes are not presented as separately
queryable accounts. The CLI fallback reads the current default sign-in and
cannot identify or deduplicate it against native services, so it runs only when
no identified account is available. Existing third-party account switchers use their
own credential isolation or replacement; Agent HUD does not rewrite those
credentials. The older `GEMINI_CLI_HOME` transcript-path descriptor is not
evidence that the CLI's authentication follows that variable.

The implemented route for identifying active CLI accounts is the official
[status-line payload](https://www.antigravity.google/docs/cli/statusline/)
contains `email`, `plan_tier`, and quota buckets with `remaining_fraction` and
`reset_time`. The ordinary
[lifecycle hook payloads](https://www.antigravity.google/docs/hooks/) do not
include those account fields. Agent HUD now installs a separate callback through
the selected CLI settings files, chains an existing user status-line command,
and restores that command on disconnect. The callback stores only account and
quota facts, with the local observation time. It merges duplicate emails and
retains each account's latest sample after switching sign-ins. Samples become
stale after five minutes and expire after 30 days. This route supports multiple
accounts without assuming a profile flag or replacing keychain credentials.

## Architecture
- Short labels and meter clamping in TS instead of Core. Short labels fixed 2026-10-20.
- Antigravity gating by a runtime string compare of a compile-time constant (acceptable).
- Antigravity approval observer runs discovery and loopback HTTP outside the collection gate (Open has an equivalent separate observer).
- Antigravity reader: three passes per change, parses gen_metadata twice, reads all steps, no watermark.
