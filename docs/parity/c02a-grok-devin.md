# c02a — Grok and Devin providers (sub-audit of c02)

Open: 15 files, 1,560 lines. A condensed report: the table lists what is missing or differs, and the matching behaviors are summarized after it.
Counts: same 18 · deliberate 2 · better 0 · missing 13 · differs 22 · unclear 1 (56 rows).

## Check of the earlier audit
- Refuted: "Grok Bot spend / routine / billing associations" — implemented in agents/src/cursor/events.rs (merge_grok_bot_spend 226, parse_automations 299, build_session_records 355; tested at 407) and agents/src/cursor/sessions.rs (refresh_routine_runs 284).
- Confirmed missing: Grok Bot replica bodies (grok_cache.rs:122 `transcript_key` unused).
- Confirmed missing: Grok CLI online quota/wallet (no grok-cli reader in accounts READERS).
- Missed by the earlier audit: Grok CLI session titles, iTerm origins, CLI renewal, tool activity, several notices, hiding billing-only sub-agents, quota refresh on Bot cache change, Devin reader window and running state from session locks.

## Missing / differs (user-visible first)
| Open | Behavior | Next | Status | What / when / user sees |
|---|---|---|---|---|
| Grok/GrokSessions.swift:19-21,61-71 | Title from summary.json (generated_title/title/session_summary), else workspace folder, else "Grok CLI"; unified-only "Grok · <8>" | agents/src/grok/transcript.rs:26-27; agents/src/transcript/mod.rs:139-146 | fixed | Every Grok CLI session titled "grok-cli:<uuid>"/"grok:<uuid>", no project **Fixed 2026-10-20** (session titles). |
| GrokClient.swift:77-131,206-226 | Grok CLI quota (billing credits), plan, account | accounts READERS has no grok-cli | missing | CLI-only users get no Grok quota/plan/account |
| GrokClient.swift:227-274 | Prepaid balance and on-demand wallets | none | missing | No Grok dollars |
| GrokClient.swift:15-75,133-157 | Bot↔CLI account links, merge, CLI fallback | src/crates/agents/src/grok/account.rs (Bot only) | missing | No merge/fallback when both signed in |
| GrokCLIRenewal.swift:8-64 | Renew expired CLI login via `grok models` with backoff | none | missing | n/a without CLI quota |
| Grok/GrokSessionOrigins.swift:6-64 | iTerm pane target from active_sessions.json | none | missing | Clicking a Grok CLI session can't reveal its iTerm pane |
| GrokSessions.swift:93-95,140-171 | Tool-call activity (shell/edit/read) | none | missing | No live Grok CLI activity in HUD/dock |
| GrokSessions.swift:107-118,136-137 | Notice: older sessions only report context size | none | missing | Zero tokens with no explanation |
| GrokSessions.swift:43-52 | Notice: overlapping log formats | none | missing | No incomplete-history warning |
| GrokBotQuota.swift:7-24 | Refresh on Bot marker/cache file change | accounts/mod.rs | missing | Bot quota stale up to ~300 s |
| GrokBotConversation.swift:5-76 | Bot transcript replica | log_files.rs:221-224 marker only | missing | No prompt/reply text in Bot session detail |
| GrokLocalSessions.swift; ProviderData.swift:12 | One "Grok" provider with two clients | core/providers.rs:37-45 two providers | deliberate | Grok quota under "Grok Bot", not one Grok group |
| ProviderData.swift:48-58 | Grok CLI detected only when ~/.grok exists | agents/src/grok/mod.rs (`CLI_DISCOVERY`) | differs | Detected via $GROK_HOME/config/PATH too |
| ProviderData.swift:53; DevinSessions.swift:10 | Devin detected only by sessions.db; desktop app must not trigger hook install | agents/src/devin/mod.rs (`DISCOVERY`); hooks/clients.rs:40-42 | deliberate | Devin Desktop alone → Devin detected and hooks written to ~/.config/devin/config.json. **2026-11-04:** intended. Devin Desktop 3.11 runs the Devin CLI it bundles (`devin acp`) as Devin Local, which reads those hooks and writes the same `sessions.db`; their origin hook names the session's client (Devin Desktop or Devin CLI). |
| GrokSessions.swift:79,113-115,205-208 | Bad records fail the file with a format notice | grok.rs:23-25,71-78 | differs | Silent skip, partial totals, no notice |
| GrokSessions.swift:66,196 | No parent-model guessing | core/ingestion.rs:348-361 | differs | Unknown-model events get an arbitrary HashMap model |
| GrokSessions.swift:86-133 | Turn re-key by promptId; start = turnStartMs ?? event time | grok.rs:41-66 | differs | Data only: turn id/duration |
| GrokBotQuota.swift:47-57 | Strict key presence; ms may be float/string | src/crates/agents/src/grok/account.rs:45-64 | differs | Normally invisible |
| GrokBotQuota.swift:69-82 | On-demand wallet row; used-without-cap extra window; short "Extra" | src/crates/agents/src/grok/account.rs:79-84; ui hud/model.ts:188-196 | differs | No On-demand line; peek "Extra usage" |
| GrokBotQuota.swift:26-35,65-68 | Notices and fallbacks | src/crates/agents/src/grok/account.rs:22-98 | differs | Different English-only messages; "did not report used credits" missing Since 2026-10-20 the notices are in Chinese and English (`l10n::text`); "did not report used credits" is still missing. |
| GrokBotSessions.swift:54-58; SessionTitle.swift | Title first non-blank line ≤60 else name else "Grok Bot"; start/last-activity | grok_bot.rs:47-58 | differs | Blank/long/multi-line titles **Title part fixed 2026-10-20**; the rest is open. |
| Cursor/CursorClient.swift:110-112 | Spend window from local midnight, max(168h, history) | agents/src/cursor/sessions.rs:332 UTC midnight | differs | Oldest day slightly off outside UTC |
| GrokBotRoutineRuns.swift:16-83 | ≤2 attempts, ≥600 s apart, 31 d, atomic save on change | agents/src/cursor/sessions.rs:284-322 | differs | Extra request every ~5 min; 30 d; non-atomic rewrite |
| GrokBotUsage.swift:56-78 | Routine child shows "Routine" tag | agents/src/cursor/events.rs:365-392 | differs | Missing tag |
| GrokBotUsage.swift:19-23; ReportView.swift:126 | Hide billing-only Bot sub-agents from lists | agents/src/cursor/events.rs:186-207 | differs | "Grok Bot subagent · xxxx" rows appear |
| GrokBotUsage.swift:61-65 | Billed sub-agents have no navigation target | checkpoints.rs:156-159; navigation.rs:129-137 | differs | Opens grokbot:// link for an unlisted agent |
| DevinQuota.swift:34,39-40 | Signed-out vs unavailable notices | src/crates/agents/src/devin/account.rs:141-144 | differs | One generic message; no signed-out state |
| DevinQuota.swift:65-66 | Source info wording | src/crates/agents/src/devin/account.rs:135-138 | differs | Wording |
| DevinSessions.swift:15-48 | 8-day window, hidden=0, ≤2000 sessions, ≤1000 nodes, 2.4 s budget | agents/src/devin/transcript.rs:11-28 | differs | Full history incl. hidden sessions in stats; full-table scan each pass |
| DevinSessions.swift:39-48 | Sessions without usage listed; title fallbacks | devin.rs:77-88 | differs | Usage-less sessions missing; "devin:<uuid>" titles **Title part fixed 2026-10-20**; the rest is open. |
| DevinSessions.swift:50-58,126-139 | Running from session_locks pid | none | missing | Quiet open Devin CLI not shown running |
| DevinSessions.swift:97-124 | Event key/model fallbacks; invalid → notice | devin.rs:33-88 | differs | "Unknown" model; possible undercount |
| DevinSessions.swift:60-61,83-91 | Missing message table still lists sessions | devin.rs:14-16 | differs | No Devin sessions at all on schema change |
| all L10n sites | Bilingual strings | Rust strings English-only | fixed | English labels in Chinese UI **Fixed 2026-10-20** (bilingual text): Rust writes its notices, statuses and reasons through `agent_hud_core::l10n::text(zh, en)` in the app's language, as Open's `L10n.text`; the host tells Core the system language and Core follows the Language setting. |
| GrokBotUsage.swift:79-82 | Billed sub-agent visibility window | not found | unclear | Need Next's session-window filter |

Same (18 rows): Bot install evidence, GROK_HOME roots, turn usage math, inference-log ownership, process generations, Bot cache framing, roster, spend API calls, daily spend rows, routine parse, bot billing join, Devin hook format, Devin quota cache selection, protobuf fields, window ids/durations, Devin roots. Deliberate: routine-runs file in Next's own data dir; Grok Bot quota absent on Windows/Linux.

Behaviors summarized above, counted for the ledger totals:

| Behaviors | Status |
| --- | --- |
| Matching behaviors listed under "Same" | same ×18 |
| Deliberate differences listed under "Same" (routine-runs file in Next's own data folder; Grok Bot quota absent on Windows/Linux) | deliberate ×2 |

## Tests
All Grok/Devin Core tests are data-in/data-out and portable to fixtures except GrokQuotaChangesTests (event/actor-bound), the file-change half of GrokAccountLinkTests, and the Devin live-pid lock test.

## Architecture
- Short window labels derived in TS (ui hud/model.ts:188-196 quotaPeekLabel) from English names instead of Core's per-window shortLabel. **Fixed 2026-10-20:** Devin "Daily"/"Weekly", Grok Bot "Weekly"/"Trial"/"Extra" come from Core.
- Grok Bot's folder comes from one place since 2026-10-22: the descriptor's `grok::account::CACHE` (macOS only), which Cursor's sessions read too.
- Grok Bot logic scattered across core and platform; roster parsing duplicated (agents/src/grok/bot.rs `read` and agents/src/cursor/sessions.rs `grok_roster`).
- Devin reader scans the whole message_nodes table with json_extract per row.
- No parallel reads.
