# c05-panel-mcp — hover-panel layout model, HUD style, MCP tools and server

## Header

**Open files covered: 36 files, 6,526 lines.**
- `Sources/AgentHUDCore/PanelLayout/`: 25 files, 5,156 lines.
- `Sources/AgentHUDCore/MCP/`: 10 files, 1,301 lines.
- `Sources/AgentHUDOpenApp/main.swift`: 69 lines.

All 36 files were read in full. The tests under `Tests/AgentHUDCoreTests/{PanelLayout,MCP}` were skimmed, but not every assertion was read.

**Path abbreviations used in the tables.**

| Prefix | Means |
|---|---|
| `P/` | `OPEN/Sources/AgentHUDCore/PanelLayout/` |
| `M/` | `OPEN/Sources/AgentHUDCore/MCP/` |
| `tools.rs` | `NEXT/src/crates/hud/src/presentation/tools.rs` |
| `presentation.rs` | `NEXT/src/crates/hud/src/presentation.rs` |
| `desk-tools.rs` | `NEXT/src/crates/host/src/tools.rs` |
| `ui/` | `NEXT/src/packages/ui/src/hud/` |
| `desk/` | `NEXT/src/apps/desktop/src/` |
| `mcp/index.ts` | `NEXT/src/apps/mcp/src/index.ts` |

**Next files consulted.**
- Tauri shell, `src/crates/host/src/`: `presentation.rs` (506), `presentation/tools.rs` (794), `tools.rs` (145), `picture.rs` (208), `mcp_clients.rs` (240), `cli.rs` (41), `screens.rs`, `lib.rs` (launch and `presentation_request`).
- Desktop app, `src/apps/desktop/src/`: `HUDPicture.tsx`, `App.tsx` (draft bar and screens wiring), `lib/use-hud-config.ts`, `settings/DisplayPane.tsx`.
- Shared UI, `src/packages/ui/src/hud/`: `HoverPanel.tsx`, `HUDScene.tsx`, `model.ts`, `panel/{QuotaSection,SessionsSection,TokenChartSection,theme}.tsx|ts`, `scene/useHUDLayout.ts`.
- MCP: `src/apps/mcp/{src/index.ts,README.md,package.json}`, `src/packages/engine-client/src/index.ts`.
- Protocol: `src/packages/protocol/{src/index.ts,hud-defaults.json,capabilities.json}`.
- Rust crates:
  - `src/crates/platform/src/local_tools/{mod,unix}.rs`
  - `src/crates/platform/src/os/macos/{mod,legacy_preferences}.rs`
  - `src/crates/platform/src/hooks/unix.rs` (the lock)
  - `src/crates/engine/src/main.rs`
  - `src/crates/core/src/sessions.rs` (session query)
  - `src/crates/core/src/presentation.rs`. Not a counterpart: it overlays session evidence.
- Docs: the earlier audit.

## Summary

### Status counts (95 rows)

| same | deliberate | better | fixed | missing | differs | unclear |
|---|---|---|---|---|---|---|
| 23 | 5 | 1 | 16 | 2 | 47 | 1 |

### The ten most user-visible problems

1. **Settings forks displays.** On any display, a change to an Expanded-panel switch, the width, the density, the sections or the order copies the shared layout into a new layout of that display's own. It also drops that display's waiting agent draft. In Open, the change goes to the shared layout and every display that uses it sees it (row 31).
2. **Revert and Reset in Settings are not persisted versions.**
   - "Revert to Previous" lives only in a React ref, so it is lost when the Settings window closes.
   - "Reset to Default" resets only the switches, width, density, sections and order. Blocks, sessions, tokens and hud customisations stay.
   - "Use Shared Layout" does not exist.
   - Rows 33–36.
3. **`draft_hud` validates almost nothing.** *Fixed 2026-10-21 (rows 13, 26, 28, 43, 65): the app refuses all of these with Open's sentences, every reason at once, and a refused call changes nothing.* Before, Next accepted all of these, which Open refuses:
   - unknown block, row, account, agent, model and project keys;
   - widgets a block's kind rejects;
   - bar rows at half span;
   - style fields the HUD's mode does not draw, and glow fields the glow style does not draw;
   - glow values between Settings' steps;
   - delays from 0 to 3000 ms, where Settings allows 0–1500 ms in 50 ms steps;
   - an appearance while matchScreen is on.

   What cannot be applied falls back or is silently ignored, and the agent is told it succeeded (rows 13, 26, 43, 63).
4. **No-op proposals leave unsaved drafts.** *Fixed 2026-10-21 (rows 14, 37, 38, 53): proposals are reduced against what is saved; a screen's proposal of the shared layout uses it.* Before, Next never reduced a proposal against what is saved. Sending back the layout, style or app that `get_hud` returned leaves an "Unsaved" bar whose Save changes nothing. Every proposal for one screen becomes that screen's own layout, so after Save the display stops following the shared layout (rows 14, 37, 38, 53).
5. **Measure picker.**
   - The picker always shows all five measures in a fixed order. Measures the layout does not list appear disabled, and a block given a single measure still gets a picker.
   - API balances lack the Tokens and Consumed measures.
   - A balance's cost covers 24 hours by default, instead of the token chart's period.
   - Rows 7–9.
6. **Line layout.**
   - Bar-row blocks pair into half lines, where Open keeps them on the whole line.
   - A block whose rows or accounts are all filtered out still draws its heading.
   - A half block keeps 4 cells per line instead of 2.
   - Rows 9, 16, 18.
7. **Edit hold.**
   - An edit of the shared layout holds the first display's panel open, not the display the person is at.
   - A dock that separates its agents opens the first agent's card, not the agent the edit touched.
   - `discard_hud_draft` closes the panel instead of holding it open.
   - Rows 48–51.
8. **`from: "previous"` behaves differently.** *Partly fixed 2026-10-21 (rows 55, 56): `from` with a layout is refused, `previous` is per scope and brings a screen's look back, and a proposal drops a style draft it does not bring. Still the in-memory snapshot of row 32.*
   - It means the in-memory snapshot taken before the last write of any kind, so it is lost on relaunch.
   - It brings back no HUD style.
   - Combined with layout fields, it merges them instead of refusing the call.
   - Rows 32, 55.
9. **Session and token data.**
   - The `ended` session state is refused by the MCP schema. *Accepted since 2026-10-21 (row 20).*
   - Projects are raw cwd strings, not repo/`~` ids, and Next has no path matching. *Fixed 2026-10-21 (row 24).*
   - The "active" sort orders running sessions by last activity, not by start time, so they reshuffle.
   - The 24-hour token chart uses 30-minute bars instead of 15-minute bars.
   - Rows 19, 20, 22, 24.
10. **Pictures and robustness.**
    - The `all` picture draws `screens[0]`'s own layout.
    - Panels taller than 2,400 pt are cropped instead of scaled down.
    - An agent's own panel is drawn at the full panel width.
    - A malformed `presentation.json` stops the app from starting.
    - Nothing prevents a second copy from launching.
    - Rows 41, 67–69, 92.

### Also worth knowing

- `get_hud` exposes raw session rows, including titles, ids and cwd, and the system display names. Open deliberately withholds both (rows 61, 63).
- Seven extra MCP tools exist. One of them, `hud_set_preference`, saves a setting immediately; in Open no tool saves (row 77).
- The earlier audit claimed a "full panel widget/filter/style/app draft contract". The code does not support that claim.
- Rust tests now cover the tools' drafts (`presentation/tool_tests.rs`, 2026-10-21): reduction, the shared-layout choice, each kind of refusal and `from`.

## Table

| # | Open (file:line) | Behavior | Next (file:line) | Status | Detail (what / when / user sees) |
|---|---|---|---|---|---|
| 1 | P/PanelLayout.swift:5-31 | Names of widgets, measures, ring sizes, spans, spacings and sections | mcp/index.ts:23-29; tools.rs:49-52,89 | same | Identical name sets. |
| 2 | P/PanelLayout.swift:197-198 | The legacy measure `tokenTotal` is read as `consumed` | presentation/rules.rs (`parse_block`) | fixed | **Fixed 2026-10-21:** `rules::normalize_layout` reads `tokenTotal` as `consumed`, as Open does, so a layout carried from an older build starts on the consumed measure. |
| 3 | P/PanelLayout.swift:36-62,76-81 | Panel width 480–720 pt, default 540; an agent's own panel is `max(440, width−100)` | tools.rs:90; ui/scene/useHUDLayout.ts:157; desk/lib/use-hud-config.ts:291 | same | Next accepts fractional widths, columns and limits in stored layouts (tools.rs:19-26). Data only; the MCP schema requires integers. |
| 4 | P/PanelLayout.swift:110-119,300-314 | Stored layouts are read forgivingly, field by field; only JSON that is not an object fails | tools.rs:27-139 | same | Unknown enums and malformed fields are dropped in the same way. |
| 5 | P/PanelLayout.swift:181-187,276-288,324-335 | Normalisation gives each layout one spelling: deduplicated, accounts and hidden rows sorted, full section order, defaults omitted, sorted keys | presentation/rules.rs (`normalize_layout`) | fixed | **Fixed 2026-10-21:** `rules::normalize_layout` writes one spelling, as `PanelLayout.normalized`: measures and rows once each in the order named, hidden rows and accounts once each in key order, the whole section order or none, hidden sections in default order, no `style:{}`, and nothing that is the default view's (countdown on, regular spacing with separators at 540, five session rows of a day in the active order, a day's chart). `hud.agents` are kept once per mark. Session and token agents are sorted but not merged by mark, because Next matches them by provider id (row 25). |
| 6 | P/PanelLayout.swift:122-175 | Block fields: widget, measures, hidden, hiddenRows, rows, accounts, countdown, columns, ringSize, span, period | mcp/index.ts:25; tools.rs:59-77; protocol/src/index.ts:325-338 | deliberate | Next adds `hiddenAccounts`. It carries Open's global hiddenAccountIDs into every block. legacy_preferences.rs:409-411 gives the reason: "Keep that as an account preference so showing the account again does not leave all its windows silently switched off." |
| 7 | P/PanelLayoutRules.swift:22-30 | Measures by kind and widget: a balance's number offers quota, burn, tokens, consumed and cost; its chart offers cost; a quota block's chart offers tokens | tools.rs:233; ui/panel/QuotaSection.tsx:654,674 | differs | **What:** a balance supports only quota, burn and cost, and `get_hud` advertises those three. **When:** any API balance block. **User sees:** no Tokens or Consumed reading on balance cards. |
| 8 | P/PanelLayoutRules.swift:51-55; P/PanelLayoutPlan.swift:10-12,38 | The picker offers only the measures the layout lists, in the order listed, and shows the first one; a single measure means no picker | ui/panel/QuotaSection.tsx:25-90,475-478,547-563 | differs | **What:** the picker always draws all five icons in a fixed order and disables the ones not listed. A single measure still gets a picker. **When:** a layout sets `measures`. **User sees:** extra disabled icons, and the agent's chosen order is ignored. |
| 9 | P/PanelLayoutRules.swift:69,251-256; P/PanelLayoutPlan.swift:149 | Bar rows cannot take half a line | ui/HoverPanel.tsx:159-172 | differs | **What:** Next pairs any two adjacent blocks set to `span: "half"`, bar rows included. **When:** a layout or proposal gives a bar block half a line. **User sees:** bar rows squeezed into half the width. Open draws them full width and refuses the proposal. |
| 10 | P/PanelLayoutRules.swift:74-79,133-135 | A balance's period defaults to the token chart's period; the menu-bar tooltip uses the same period | ui/panel/QuotaSection.tsx:651-655 | differs | **What:** the default is always `day`. **When:** `tokens.period` is week or month and the balance sets no period. **User sees:** the balance's cost and burn cover 24 hours instead of 7 or 30 days. |
| 11 | P/PanelLayoutRules.swift:123-127,142-159 | Block keys and default order: one quota block per vendor in row order, then `api:` + (billingPool.provider ?? vendor); kind is `billing` | tools.rs:205-255 | differs | **What:** `get_hud` lists blocks alphabetically, because serde_json is built without `preserve_order` (Cargo.lock). The kind is spelled `"balance"`. Pooled accounts are keyed by vendor, not by pool provider. **When:** every `get_hud`. **User sees:** nothing in the panel, but the agent's order and kind do not match it. **Update 2026-10-21:** get_hud now lists blocks in the panel's default order (`rules::present_blocks`), which the reduction uses too. Still: the kind is spelled `"balance"` and pooled accounts are keyed by vendor. |
| 12 | P/PanelLayoutRules.swift:84-99,145-155 | An account's id is ProviderAccount.id or APIBilling.id; rows that belong to no account have an empty id that no layout can name | tools.rs:215-218; ui/model.ts:232 | differs | **What:** the id is `accountId ?? account label ?? "unassigned"`. A label used as the id changes when the label changes, and `unassigned` can be named. **When:** accounts have no id. **User sees:** nothing directly (data only). |
| 13 | P/PanelLayoutRules.swift:183-310 | A proposal is checked against the blocks: unknown block or order keys, a widget the kind does not accept, a measure not offered, countdown on a balance, columns, ring size or period where none applies, half span for bar rows, unknown rows or accounts (what a held layout keeps is exempt); the whole proposal is refused with every reason | presentation/rules.rs (`issues`); presentation/tools.rs (`draft`) | fixed | **Fixed 2026-10-21:** `rules::issues` ports the checks and Open's sentences, with what the scope's effective and saved layouts hold for a block, row or account that is away exempt; the whole proposal is refused with every reason and nothing changes. Next's differences: `hiddenAccounts` is checked like `accounts`; a balance's card offers quota, burn and cost, what Next draws on it (row 7), so tokens and consumed on a balance are refused with a sentence that says so; a hidden row may be named by any key the panel matches it by (`quotaRowMatches`: its id, its legacy id, or its window key). |
| 14 | P/PanelLayoutRules.swift:327-364; P/PanelLayoutStore.swift:152-167; P/HUDTools.swift:305-314 | A proposal is reduced to what it changes; a proposal equal to the saved layout clears the draft | presentation/rules.rs (`reduced`); presentation/tools.rs (`propose_shared`) | fixed | **Fixed 2026-10-21:** `rules::reduced` reduces a proposal against the blocks present now, as `reduced(against:)` does; a proposal that draws what is saved is the saved layout and clears the draft, with Open's note ("This is the layout already saved, so there is no draft."). A no-op call no longer bumps the revision. One deliberate step from Open: a balance's default period is the day, which is what Next draws (row 10), so a balance's `period: "week"` is never reduced away while Next would draw a day. |
| 15 | P/PanelLayoutPlan.swift:63-67,106-108 | Blocks follow `order` first, then the default order; rows follow `rows` first, then their own order | ui/panel/theme.ts:63-72; ui/HoverPanel.tsx:110-127 | same | Same stable ordering. |
| 16 | P/PanelLayoutPlan.swift:123-131 | A block is left out when it is hidden, when its accounts filter keeps none, or when every kept row is hidden | ui/HoverPanel.tsx:91-97,110-127; ui/panel/QuotaSection.tsx:658-668 | differs | **What:** Next drops only `hidden` blocks. **When:** `hiddenRows` or `accounts` cover everything. **User sees:** a block with a heading and a picker but no rows; for a balance, an empty line with a separator. |
| 17 | P/PanelLayoutPlan.swift:103-111; P/PanelLines.swift:41-50 | Two halves share a line when (content − blockGap)/2 ≥ 230; content defaults to width − 36; never inside an agent's own panel | ui/HoverPanel.tsx:164 | differs | **What:** the threshold is the same, but the fallback content width is width − 40. Pairing also happens in a focused agent's panel. **When:** two half blocks of one vendor are shown in its own card. **User sees:** the halves side by side, where Open stacks them. |
| 18 | P/PanelLines.swift:93-99; P/PanelLayoutPlan.swift:23-25 | A half block sets 2 cells per line by default, capped at what fits with cells of at least 58.7 pt | ui/panel/QuotaSection.tsx:319 | differs | **What:** columns default to `layout.columns ?? 4`, with no cap. **When:** a ring or number block takes half a line. **User sees:** 4 cramped cells instead of 2. |
| 19 | P/PanelLayoutFilters.swift:19-49; Logic/ChartData.swift:180 | Token periods: 24 hours in 15-minute columns, 7 and 30 days in days | ui/panel/TokenChartSection.tsx:27-32,50 | differs | **What:** the 24-hour chart uses 30-minute buckets. **When:** always. **User sees:** 48 bars instead of 96. |
| 20 | P/PanelLayoutFilters.swift:7-16 | Session states are waiting, running and ended; idle and unverified count as ended | hud/presentation/rules.rs (`session_misreadings`) | deliberate | **Owner 2026-10-26:** the panel and the Sessions page list sessions newest last message first (the person's prompt or the agent's words, `lastMessageAt`) and filter or label none by state; `sessions.states` and `sessions.sort` are gone, and a saved layout reads without them. Core still works out the state for agents, the phone and the bots. |
| 21 | P/PanelLayoutFilters.swift:55-81 | Session period is day or week; legacy names are mapped (1h–24h → day, 3d and 7d → week) | tools.rs:119-127; ui/panel/SessionsSection.tsx:41 | differs | **What:** legacy names are kept verbatim, and anything that is not `week` counts as a day. **When:** a layout saved by an older build. **User sees:** a list that covered a week now covers a day. |
| 22 | P/PanelSessionRows.swift:100-127 | Session rows: the list covers in-flight sessions plus those ended in the period; at most `limit` rows, 5 by default; "+N more"; sort `active` by default | ui/panel/SessionsSection.tsx; core/sessions.rs | deliberate | The scope maps day → 1 day and week → 7 days; in-flight sessions are still included. **Owner 2026-10-26:** the panel and the Sessions page list sessions newest last message first (the person's prompt or the agent's words, `lastMessageAt`) and filter or label none by state; `sessions.states` and `sessions.sort` are gone, and a saved layout reads without them. Core still works out the state for agents, the phone and the bots. |
| 23 | P/PanelSessionRows.swift:112-123 | The `active` sort puts waiting first, then running, newest **started** first within each, then ended by last event | core/sessions.rs; ui/panel/SessionsSection.tsx | deliberate | **Owner 2026-10-26:** the panel and the Sessions page list sessions newest last message first (the person's prompt or the agent's words, `lastMessageAt`) and filter or label none by state; `sessions.states` and `sessions.sort` are gone, and a saved layout reads without them. Core still works out the state for agents, the phone and the bots. |
| 24 | P/PanelSessionProjects.swift:4-82 | A project id is its folder written with `~`, with a repository's worktrees counted as one project, or `unassigned`; a path inside a project names it (innermost, links resolved, as git places it); a project named by its name is refused | presentation/rules.rs (`offered_projects`, `project_match`); core/projects.rs | fixed | **Fixed 2026-10-21:** sessions carry Core's project id (`projects::project_id`: the folder with `~`, a repository's worktrees as one); get_hud offers each project once, by id, in Finder's order with `unassigned` last; a proposal may name a project by its id or by a path inside one (as written or as Git places it, links resolved), which is listed by its id with Open's note; a project named by its name, or a path inside none, is refused with Open's sentences. The projects offered are those of the snapshot's sessions. |
| 25 | P/PanelSessionRows.swift:29-33; P/PanelLayoutFilters.swift:108-111 | `sessions.agents` and `tokens.agents` are matched by mark: "Codex" covers Codex and ChatGPT | ui/panel/SessionsSection.tsx:30-34; ui/model.ts:224-228; ui/panel/TokenChartSection.tsx:56 | differs | **What:** each name maps to one exact provider id. Mark matching happens only in an agent's own panel. **When:** a vendor name covers more than one client. **User sees:** the sibling client's sessions or tokens are missing. |
| 26 | P/PanelLayoutFilters.swift:329-369; P/PanelHUD.swift:94-107 | Agents, models and HUD agents that are absent now are refused, and so is a model whose agent the filter leaves out | presentation/rules.rs (`filter_issues`, `hud_issues`, `resting_agents`); tools.rs (`token_catalog`) | fixed | **Fixed 2026-10-21:** `rules::filter_issues` and `rules::hud_issues`, with Open's sentences: `sessions.agents` against the clients with sessions now (a provider's id or name, as the panel matches them, row 25); `tokens.agents` and `tokens.models` against the ledger catalog of the chart's longest period, 30 days, which get_hud now offers too; a model whose provider `tokens.agents` leaves out; and `hud.agents` against what the screen's resting HUD shows in the mode it would have (`resting_agents`, a port of resting-agents.ts), or any attached screen's for the shared layout. What a held layout already lists is exempt. Without a readable catalog, token agents and models are not checked. |
| 27 | P/PanelSessionRows.swift:131-144 | Offered values: session agents are the clients that have sessions now, merged by mark and sorted; models are the report's consumers | tools.rs:327; desk-tools.rs:124-131 | differs | **What:** session agents are every provider. Models come from today's UTC ledger catalog. **When:** every `get_hud`. **User sees:** nothing directly (data only); the agent gets mismatched choices. **Update 2026-10-21:** session agents are now the clients with sessions (sorted, not merged by mark, row 25), and models and token agents come from the ledger catalog of the last 30 days, the chart's longest period; a proposal is checked against the same values (row 26). |
| 28 | P/PanelLayoutMisreadings.swift:11-138; P/HUDStyleRules.swift:10-109 | Every value the reader would forgive is refused in a proposal, with a precise sentence; null means not given | presentation/rules.rs, style_rules.rs, app_rules.rs; mcp/index.ts | fixed | **Fixed 2026-10-21:** the relay advertises the zod schema but no longer validates `draft_hud` (mcp/index.ts `checkedByTheApp`); the app refuses what reading would forgive with Open's sentences (`rules::misreadings`, `style_rules::misreadings`, `app_rules`), Open's hints included (`style.appearance` → `hud.appearance`), unknown arguments as "Unknown: … It takes: …", and null means not given. Next has no `hud.material` or `hud.glass` (c07 #57). |
| 29 | P/PanelHUD.swift:52-85 | `hud.agents` narrows the HUD only when at least one listed agent is present, in the listed order | ui/model.ts:215-221 | same | — |
| 30 | P/PanelSwitches.swift:62-83 | The switches are the layout's hidden sections plus its `countdown` | tools.rs:748-771; desk/lib/use-hud-config.ts:289-294 | same | — |
| 31 | P/PanelLayoutStore.swift:376-402 | A switch in Settings sets the layout the display keeps saved (its own, else the shared one) and applies to a waiting draft too; it creates no version | presentation.rs:313-330; desk/settings/DisplayPane.tsx:455-460 | differs | **What:** Next forks the shared layout into a new own layout for the selected display, drops that display's layout draft, and records a whole-state "previous". **When:** any Expanded-panel switch, width, density, section or order change. **User sees:** the change reaches only that display, and an agent's draft vanishes. |
| 32 | P/PanelLayoutStore.swift:17-57; P/PanelLayoutStorage.swift:49-56 | Each document (shared, and each screen's own) keeps a saved and a previous version, and the previous one is persisted | presentation.rs:47-58,127,149 | differs | **What:** the previous version is a one-step snapshot of the whole state, in memory only. **When:** after a relaunch, or after any unrelated settings write. **User sees:** "previous" is gone, or equals the current version. |
| 33 | P/PanelLayoutStore.swift:311-326 | Revert to Previous swaps the saved and previous versions, persisted, look included | desk/settings/DisplayPane.tsx:92-138,170-172 | differs | **What:** Next keeps the previous version in a React `useRef`. **When:** the Settings window is reopened or reloaded. **User sees:** Revert is greyed out. |
| 34 | P/PanelLayoutStore.swift:331-343 | Reset to Default sets `PanelLayout()` whole | desk/settings/DisplayPane.tsx:64-73,109-122 | differs | **What:** only switches, width, density, sections and order are reset. **When:** Reset is pressed. **User sees:** custom widgets, filters, hud agents and token and session settings remain. |
| 35 | P/PanelLayoutStore.swift:361-367 | Use Shared Layout for a display | — (MCP `from:"shared"` only) | missing | **User sees:** Settings offers no way back to the shared layout. |
| 36 | P/PanelLayoutStore.swift:227-266,345-357; P/HUDStyle.swift:194-234 | A screen's versions cover its look: the previous version keeps placement and glow; Revert and Reset restore them; `from:"previous"` brings the style back | desk/settings/DisplayPane.tsx:96-130; tools.rs:556-570 | differs | **What:** style revert is a React ref, and the agent's `previous` carries no style. **User sees:** the HUD style cannot be reverted after Settings closes. |
| 37 | P/PanelLayoutStore.swift:176-181; P/HUDStyle.swift:182-191 | A style draft is reduced against what is saved; an empty one, or a reset when the screen is already default, leaves no draft | presentation/style_rules.rs (`proposal`); presentation/tools.rs (`propose_style`) | fixed | **Fixed 2026-10-21:** a style is reduced against the screen's saved look (`style_rules::reduced`): the listed style sent back, a waiting draft's style sent back, or `reset` on a screen that already draws the defaults leaves no new draft. A proposal that changes nothing drops the style draft. |
| 38 | P/PanelLayoutStore.swift:194-205; P/AppDraft.swift:73-78 | An app draft is reduced against what is saved | presentation/app_rules.rs | fixed | **Fixed 2026-10-21:** `app_rules::proposal` reduces the app against what is saved; get_hud's `app` sent back, read-only fields included, leaves no draft, and the note says so. |
| 39 | P/PanelLayoutStore.swift:199-205; P/AppDraft.swift:150-163 | A person's change to a setting in Settings drops that field from the app draft | presentation.rs:79-112,289-296 | same | — |
| 40 | P/PanelLayoutStore+Bar.swift:10-27; P/PanelLayoutStore.swift:116-126 | The bar's Save and Discard act on the shared draft only while the display draws the shared layout | presentation.rs:449-453,486-491 | differs | **What:** Next falls back to the shared draft whenever the display has no layout draft of its own, even when it keeps its own saved layout. **When:** a display with its own layout has a style or app draft while a shared draft waits. **User sees:** Save or Discard also saves or drops a shared draft that display never showed. |
| 41 | P/PanelLayoutStorage.swift:58-81 | Reading stored layouts forgives side by side; nothing unreadable loses the rest | presentation.rs:114-122; lib.rs:728-733 | differs | **What:** a malformed `presentation.json` makes `Presentation::new` return an error, and setup fails. **When:** the file is corrupted or wrongly typed. **User sees:** the app does not start. |
| 42 | P/PanelLayoutStorage.swift:49-143 | The storage format in UserDefaults | presentation.rs:23-46,138-153 | deliberate | #2: Next keeps its own `presentation.json` and imports the legacy file once (legacy_preferences.rs:503-655). |
| 43 | P/HUDStyleRules.swift:124-160 | A screen's style is checked against what it would draw: `notch` without a notch, fields the resulting mode does not draw, a glow on Simple, glow fields of another glow style, amplitude outside breathe, glow steps (pitch, core and fade whole; density 0.05; idle 0.5 s) | presentation/style_rules.rs (`issues`) | fixed | **Fixed 2026-10-21:** `style_rules::issues` ports the checks and sentences: `notch` without a notch, fields the resulting mode does not draw, a glow on Simple, glow fields of another glow style, amplitude outside breathe, and glow steps (pitch, core and fade whole, density 0.05, idle 0.5 s). They are asked of the reduced style, so a field that repeats what is saved is none. The default mode of a screen without a notch is a dock, as the panel draws it. |
| 44 | P/HUDStyleRules.swift:166-172 | A notch HUD made a dock at the top centre starts at position 0, clear of the notch | presentation/style_rules.rs (`placed_clear_of_the_notch`) | fixed | **Fixed 2026-10-21:** as `placedClearOfTheNotch`, with Open's note. |
| 45 | P/HUDStyle.swift:9-16,121-176,255-330; P/HUDStyleRules.swift:10-41 | Style field names, their mapping to placement and glow fields, numeric ranges, glow styles and effects, edges | tools.rs:336-523; mcp/index.ts:27; protocol/hud-defaults.json | same | Every range matches Open's ScreenPlacement and GlowSettings (20–60, 0.1–0.6, 0–20, 1–4, 0–36, 0.2–1, 1–12, 1–24, 4–12, 0–8, 0.5–1.5). |
| 46 | P/HUDStyle.swift:40-46,139-140,404-418 | `transparency` sets the transparency of the material in effect; `glassStyle`; the kept solid and glass transparencies | style_rules.rs (`FIELDS`); mcp/index.ts | deliberate | Next has no glass (decided 2026-10-21; c07 #57): `transparency` always sets the solid's transparency (`solidTransparency`), and there is no `material`, `glassStyle` or kept glass transparency. |
| 47 | P/HUDTools+Style.swift:35-114 | Style rules: none for the shared layout, `reset` excludes other fields, a waiting style draft is kept | tools.rs:632-670 | same | Same refusals, different wording. **Update 2026-10-21:** the wording is Open's too. |
| 48 | P/HUDTools+Hold.swift:14-22 | An edit of the shared layout holds the panel of the display the person is at, if it draws the shared layout, else the first display that does | tools.rs:620-630 | differs | **What:** Next always holds `screens[0]`. **When:** the person is at another display, or the first display has its own layout. **User sees:** the wrong panel pops open, or one that does not show the change. |
| 49 | P/HUDTools+Hold.swift:37-49; P/PanelHolds.swift:27-29 | A held dock opens the card of the agent the edit touched first | ui/HUDScene.tsx:191-200 | differs | **What:** Next opens the first agent. **When:** a dock that separates its agents, and an edit of a block that is not the first. **User sees:** the wrong agent's card is held open. |
| 50 | P/HUDTools.swift:126-134 | `discard_hud_draft` holds the panel open, unless it drops a style alone | tools.rs:697-705 | differs | **What:** Next ends the hold, and for `all` it ends every hold. **When:** a discard. **User sees:** the panel closes at once instead of showing the restored layout. |
| 51 | P/PanelHolds.swift:68-73; P/HUDTools.swift:320-321 | A call that changes the style alone pauses the hold; `holdPaused` is reported | tools.rs:674-676,313-318 | differs | **What:** Next ends the hold instead and reports no `holdPaused`. **When:** a style-only call. **User sees:** the same panel close; `get_hud` reports `held:false`. |
| 52 | P/PanelHolds.swift:19,62-66 | A hold lasts 120 s from the last edit and each edit renews it | tools.rs:629 | same | — |
| 53 | P/HUDTools.swift:364-372 | A screen's proposal equal to the shared layout becomes "use shared"; one equal to its own becomes its own | presentation/tools.rs (`choice`, `propose_choice`) | fixed | **Fixed 2026-10-21:** `tools::choice` as `HUDTools.choice`: the screen's own saved layout where the proposal draws that; the shared layout where the screen draws the shared layout, saved or as its draft, and the proposal draws what that draws; else an own layout. A choice equal to what is saved clears the draft. |
| 54 | P/HUDTools.swift:174-185 | `screen` may be null (the shared layout), `all`, an attached screen, or a screen away that keeps a layout | presentation/tools.rs (`scope`) | fixed | **Fixed 2026-10-21:** `screen` null is the shared layout; a screen that is away but keeps a layout or a draft can be named; refusals use Open's sentences and name screens by role. |
| 55 | P/HUDTools.swift:229-273 | `from`: default, previous or shared; `from` with a layout is refused; `previous` is per scope; the errors that apply | presentation/tools.rs (`draft`) | differs | **Partly fixed 2026-10-21:** `from` with any layout key is refused ("Pass either from or a layout, not both."), as are an unknown `from`, `shared` for the shared layout, and a missing previous version, with Open's sentences. `previous` is per scope: the shared layout, or the screen's own layout or use of the shared one, and for an attached screen its look. **Still:** `previous` is the in-memory snapshot taken before the last write of any kind (row 32), so it is lost on relaunch or can equal the current version. |
| 56 | P/HUDTools+Style.swift:125-135; P/HUDTools.swift:244,261 | A proposal is whole: a style draft it does not bring is dropped, including with `from` default or shared | presentation/tools.rs (`draft`, `propose_style`) | fixed | **Fixed 2026-10-21:** a layout proposal for a screen drops a style draft it does not bring, and so do `from` default and shared; `from` previous brings the version's look back as the style draft. |
| 57 | P/HUDTools.swift:402-472; P/HUDTools+App.swift:81-96; P/HUDTools+Reading.swift:65-76; P/HUDTools+Hold.swift:60-84 | Notes for every outcome: draft or saved, screens that do not show it, style and app notes, a dock placed clear of the notch, glass kept, project paths, halves that took a whole line, finish | tools.rs:695,710,723 | differs | Next returns one fixed sentence per tool. Text only. **Update 2026-10-21:** draft_hud's notes are Open's: saved or draft, screens with a layout of their own that do not show a shared draft, the style draft, the app draft, a dock placed clear of the notch, and a call with the style or the app alone. Still: discard and finish keep one fixed sentence each. |
| 58 | P/HUDTools+Reading.swift:28-63 | `get_hud` top-level fields: hasPrevious, style, countdown, sections, usesSharedLayout, height, scrolls, … | tools.rs:256-333 | differs | **What:** those are missing, and the internal store, layouts, sources, drafts and holds are dumped instead. **When:** every `get_hud`. **User sees:** nothing; the agent cannot learn the panel's height or whether it scrolls. |
| 59 | P/HUDTools+Reading.swift:16-18 | `unsaved` is true exactly when the panel shows the draft bar: its own or the shared draft, a style draft, or the app draft | tools.rs:325; desk/App.tsx:518 | differs | **What:** `unsaved` is false for a display that draws the shared draft or shows only an app draft, although the bar shows there. **User sees:** nothing; the agent misreads the state. |
| 60 | P/HUDTools+Reading.swift:107-173,224-255 | A block entry gives shown, the widget, measures, offered measures, countdown, columns, ring size, period and span in effect, its name, its accounts, and readings in the order drawn | tools.rs:205-255 | differs | **What:** Next returns raw snapshot rows and raw layout settings, with no resolved values and no `shown`. **When:** every `get_hud`. **User sees:** nothing (data only). |
| 61 | P/HUDTools+Reading.swift:175-200 | Sessions: the filter in effect, offered values, shown, more and byState counts; no titles or session ids | tools.rs:327 | differs | **What:** raw session rows, with titles, ids and cwd, plus the raw filter; no counts. **When:** every `get_hud`. **User sees:** nothing, but session titles now reach the agent (privacy). |
| 62 | P/HUDTools+Reading.swift:202-218 | `tokens.total` sums the chart over its period after the filters | tools.rs:328 | differs | **What:** the snapshot's totals, whatever the period or filter. Data only. |
| 63 | P/HUDTools+Reading.swift:78-101; P/PanelScreen.swift:5-48; P/HUDTools+Style.swift:150-174 | A screen entry: its role name ("Built-in display", "Main display", "Display N"), layout shared or own, width, agentPanelWidth, unsaved, hud, look, styleUnsaved, agents, shows, hold | tools.rs:259-319; desk/App.tsx:237 | differs | **Partly fixed 2026-10-21:** a screen without a notch now lists the mode it draws, a dock, and sentences name screens by role ("Built-in display", "Main display", "Display N"). **Still:** the listing carries the raw monitor objects, the system display name included, and no role, layout, widths, agents, shows or look. |
| 64 | P/HUDTools+App.swift:24-61; P/AppDraft.swift:17-196 | App fields and names, `hover` nesting, `language` and `writtenIn` read only | tools.rs:160-204,679-693; mcp/index.ts:28 | same | `writtenIn` is computed from `language.startsWith("zh")`, so `system` does not resolve to the system language. Minor. |
| 65 | P/AppDraft.swift:262-279 | App rules: delays 0–1500 ms in steps of 50; appearance refused while matchScreen is on | presentation/app_rules.rs; mcp/index.ts | fixed | **Fixed 2026-10-21:** delays 0–1500 ms in steps of 50 and an appearance while matchScreen is on are refused with Open's sentences, of what the draft changes, so a delay saved outside the range and sent back is none. The schema advertises 0–1500 in steps of 50. |
| 66 | P/HUDTools.swift:216-224; P/PanelLayoutStore.swift:446-476; P/HUDTools.swift:41-43 | The revision guard: the store's changes plus a random launch offset | tools.rs:539-543; presentation.rs:151,294,301 | differs | **What:** Next counts from 0 every launch. The revision also bumps on display-list changes, on any settings write, and on no-op drafts. **When:** a display reconfigures, or the app relaunches. **User sees:** nothing; the agent gets spurious "changed" refusals, or a stale revision is accepted. **Update 2026-10-21:** a tool call that changes no draft no longer bumps the revision. Still: no launch offset, and display-list changes and settings writes bump it. |
| 67 | P/HUDTools+Picture.swift:12-62; P/PanelPicture.swift:28-57 | The panel picture shows the panel whole, at 2× unless taller than 2,400 px (then scaled down); fields width, height, scrolls, pixelsPerPoint, note | desk/HUDPicture.tsx:30-39; picture.rs:54,198-203 | differs | **What:** the height is capped at 2,400 **pt** and the rest is cropped. Next reports pixel sizes but no `scrolls` or scale. **When:** a tall panel. **User sees:** nothing directly; the agent's picture is cut off. |
| 68 | P/HUDTools+Picture.swift:29-31 | The shared-layout picture draws the shared layout in the main display's look | desk/HUDPicture.tsx:24-25; desk/lib/use-hud-config.ts:278 | differs | **What:** Next renders `screens[0]`'s config, its own layout if it has one. **When:** `screen` is all, after any Settings change (row 31). **User sees:** nothing; the agent sees the wrong layout. |
| 69 | P/HUDTools+Picture.swift:111-167 | An agent's own panel is drawn at `agentPanelWidth`; the agent is matched by mark or block key; the refusals that apply | picture.rs:44-52; desk/HUDPicture.tsx:30,33 | differs | **What:** Next draws at the whole panel width. **User sees:** nothing; the agent's picture misrepresents the card. |
| 70 | P/HUDTools+Picture.swift:175-230 | The resting-HUD picture: fields mode, edge, appearance, material, glow; notes such as "black at rest" | picture.rs:54; desk/HUDPicture.tsx:28,45 | differs | Only metadata (screen, revision, unsaved, note). Data only. |
| 71 | P/PanelPicture.swift:86-105 | The account mask `j***s@domain`; a session row is "会话 N" or "Session N" | desk/HUDPicture.tsx:15-16,21 | same | The mask is identical. Session labels are always English, and notices are dropped instead of drawn as grey bars. Minor. |
| 72 | M/MCPTools.swift:10-15 | The five tool names | mcp/index.ts:33-37 | same | — |
| 73 | M/MCPTools.swift:17-24; M/MCPServer.swift:116-117,145-147 | The server's instructions, and serverInfo `agent-hud` with the app's version | mcp/index.ts:31 | differs | **What:** The server reports `agent-hud` 0.1.0 but sends no `instructions`. **When:** a client connects. **User sees:** nothing; the agent lacks the "draft until Save / finish_hud_edit" primer. |
| 74 | M/MCPTools.swift:28-284 | Detailed tool and field descriptions with ranges | mcp/index.ts:22-37 | differs | Short descriptions; most fields have none. The agent gets less guidance. |
| 75 | M/MCPTools.swift:111-284 | The `draft_hud` schema: no additional properties; every field optional | mcp/index.ts | fixed | **Fixed 2026-10-21:** null values and `ended` are accepted, delays are advertised as 0–1500 in steps of 50, and the app checks the proposal (row 28). What remains is deliberate: `hiddenAccounts` (row 6), and no `material`, `glassStyle` or kept transparencies (row 46). |
| 76 | M/MCPTools.swift:290-298 | Annotations | mcp/index.ts:8-9 | same | Read-only tools also carry `idempotentHint` and `destructiveHint`. Harmless. |
| 77 | — | Only five tools; no tool saves | mcp/index.ts:38-85; desk-tools.rs:102-115 | differs | **What:** seven extra tools. `hud_set_preference` **saves** appearance or animations immediately, and `hud_snapshot` returns the whole snapshot, session titles included. **User sees:** an agent can change the appearance without the user's Save. |
| 78 | M/MCPServer.swift:13-168; M/MCPProtocol.swift:10-73 | JSON-RPC and revisions: 2024-11-05 to 2025-11-25 with a handshake, 2026-07-28 stateless, `server/discover`, `subscriptions/listen`, batches, ttlMs and cacheScope, error codes, unknown tool → -32602 | mcp/index.ts:1-2,30 (SDK `@modelcontextprotocol/server` 2.3.1) | unclear | Needs the SDK's supported revisions and its stateless handling; `node_modules` is not in the snapshot. |
| 79 | M/MCPServer.swift:175-211 | A tool result is one text block of JSON plus image blocks, and `isError` | mcp/index.ts:10-20 | same | Next adds `structuredContent`, and its keys are unsorted. |
| 80 | M/MCPServer.swift:200-203; M/MCPAppClient.swift:24-40 | Distinct messages for "not running", "tools are off", "no answer" and "unreadable" | platform/local_tools/unix.rs:162-163; desk-tools.rs:91-100 | differs | Next merges "not running" and "off" into one sentence (Open tells them apart by the instance lock). Text only. |
| 81 | M/MCPToolCall.swift:17; M/MCPAppClient.swift:18-22 | A request may be up to 1 MB and a reply up to 16 MB; the timeout is 10 s | platform/local_tools/mod.rs:9-10; unix.rs:107-108,168; engine-client/src/index.ts:49; picture.rs:104 | differs | **What:** requests are limited to 128 KiB. Timeouts are 40–45 s, and 25 s for a picture. **When:** a large `draft_hud`, or the app is slow. **User sees:** nothing; the agent's call is refused, or it waits longer. |
| 82 | M/MCPAppChannel.swift:11-55 | One socket in the data directory, handled one call at a time | platform/local_tools/unix.rs:52-138 | same | Same model, serial accept loop. The private 0700 directory and lock are hardening. |
| 83 | M/MCPStdio.swift:12-118 | `--mcp` only as the sole argument; one process with a 4 MB line buffer, CR trimmed, exits on EOF | desk cli.rs:19-41; engine/src/main.rs:10-36 | differs | **What:** `--mcp` is accepted anywhere and starts a Node relay and an engine `--app-channel` process; the SDK owns the framing. Minor. |
| 84 | M/MCPClient.swift:11-74 | Add and remove arguments, server name `agent-hud`, settings JSON | mcp_clients.rs:104-125,137 | same | Next shell-quotes every argument in the displayed command and allows setup only from the canonical installed executable (mcp_clients.rs:129-149). |
| 85 | M/MCPClient.swift:81-89 | Finding the client: Claude in `~/.local/bin`, `~/.claude/local`, Homebrew, `/usr/local`, `~/.local/share/claude/versions`, PATH; Codex in the app's `codex-cli/bin/codex`, registered apps, then the CLI | mcp_clients.rs:29-66 | differs | **What:** Next misses `~/.claude/local/claude`, the versions directory, Codex.app's `Contents/Resources/codex-cli/bin/codex` and LaunchServices apps. **When:** clients installed there. **User sees:** "The client's command line is not installed". |
| 86 | M/MCPClient.swift:93-123; M/MCPTOMLTables.swift:6-111 | Whether the server is configured: Claude's user `mcpServers`; Codex by a tolerant TOML scan | mcp_clients.rs:67-103 | differs | **What:** Next parses the whole TOML; any syntax error elsewhere, or a file over 4 MB, means "not installed". **User sees:** a wrong "not added" status. Minor. |
| 87 | M/MCPClientCommand.swift:36-58 | Run with a PATH prefix, from the home folder, stopped after 15 s, with a one-line failure | mcp_clients.rs:140-214 | same | Wording differs: the first line is not trimmed and no "…" is added. |
| 88 | P/PanelSwitches.swift:43-59; P/PanelLayoutStore.swift:409-424 | Legacy switches are carried into every saved layout | legacy_preferences.rs:381-395 | deliberate | Part of the one-time import (#2). The previous versions are not imported. |
| 89 | main.swift:57-58 | Hook commands run before the UI starts | lib.rs:686-691 | same | — |
| 90 | main.swift:53,68 | The accessory policy; the app keeps running with no window | lib.rs:438-449,752-757 | same | Both switch to the regular policy while a window is open. |
| 91 | main.swift:21-34,40-43 | Demo uses its own defaults suite and agents | lib.rs:694,722-737 | deliberate | #3. |
| 92 | main.swift:17-24 | A single instance: a second launch says where the running copy is and quits | platform/src/instance/{mod,unix,windows}.rs; host/src/instance.rs | better | Same as c03 #83 and c09 #3 (fixed 2026-10-20): the shared lock is claimed before Tauri starts, a refused copy names the running one and quits, and a second launch of the same executable brings the running copy's Settings forward. |
| 93 | main.swift:10-16,26-38,60-63 | `--snapshot`, `--reset-defaults`, `--probe`, `--probe-open-agents` | — | missing | Developer and diagnostic flags. Not user-visible. |
| 94 | P/PanelLayoutPlan.swift:139-157 | A block's countdown is its own, else the layout's; quota blocks only | ui/panel/QuotaSection.tsx:624 | same | — |
| 95 | P/PanelLayoutPlan.swift:113-117 | Sections in order with hidden ones left out; spacing gaps 6/10/14; separators | desk/lib/use-hud-config.ts:278-295; ui/panel/theme.ts:74-79; ui/HoverPanel.tsx:173-244 | same | — |

## Tests

| Open test file | Covers | Portable to fixtures? |
|---|---|---|
| PanelLayout/PanelLayoutTests.swift | PanelLayout.swift; misreadings | Yes. Pure JSON in and out. |
| PanelLayout/PanelLayoutRulesTests.swift, PanelBlockRowsTests.swift | PanelLayoutRules.swift (kinds, issues, reduced, accounts and rows) | Yes. A blocks fixture plus a layout gives issues and the reduced layout. |
| PanelLayout/PanelLayoutPlanTests.swift, PanelLayoutSpanTests.swift | PanelLayoutPlan.swift, PanelLines.swift | Yes. A layout, blocks and a width give a plan. |
| PanelLayout/PanelLayoutFiltersTests.swift | PanelLayoutFilters.swift, PanelSessionRows.swift, PanelSessionProjects.swift | Mostly. Project path matching needs filesystem and git fixtures. |
| PanelLayout/PanelHUDTests.swift | PanelHUD.swift | Yes. |
| PanelLayout/PanelSwitchesTests.swift | PanelSwitches.swift; store `setSwitches` and `carry` | Yes, as action-sequence fixtures. Uses UserDefaults in Swift. |
| PanelLayout/PanelLayoutStoreTests.swift, PanelLayoutScreenStoreTests.swift, PanelLayoutResetTests.swift, PanelLayoutStyleDraftTests.swift | PanelLayoutStore.swift, +Bar | Yes, as state-machine transcripts. UserDefaults- and SettingsStore-bound in Swift. |
| PanelLayout/PanelLayoutPersistenceTests.swift | PanelLayoutStorage.swift | The bytes are portable, but Next deliberately uses its own format (#2). Reuse it for the legacy import. |
| PanelLayout/HUDStyleTests.swift | HUDStyle.swift, HUDStyleRules.swift | Yes. Pure. |
| PanelLayout/HUDToolsStyleTests.swift, HUDToolsStyleAloneTests.swift, HUDToolsLookTests.swift | HUDTools+Style.swift | Yes, as tool request → JSON fixtures given settings and a screens fixture. Look needs the system light and Reduce Transparency inputs. |
| PanelLayout/HUDToolsHoldTests.swift | PanelHolds.swift, HUDTools+Hold.swift | Yes, with an injected clock. |
| PanelLayout/HUDToolsAppTests.swift | AppDraft.swift, HUDTools+App.swift | Yes. Partly relaunch- and UserDefaults-bound. |
| PanelLayout/HUDToolsTests.swift, HUDToolsAccountsTests.swift, HUDToolsBalanceTests.swift, HUDToolsOptionsTests.swift, HUDToolsResetTests.swift, HUDToolsSpanTests.swift, HUDToolsFiltersTests.swift, HUDToolsScreensTests.swift (base HUDToolsCase.swift) | HUDTools.swift, HUDTools+Reading.swift, PanelScreen.swift | Yes, as request → response fixtures over a DemoData store snapshot. Needs a shared snapshot fixture. |
| PanelLayout/HUDToolsPictureTests.swift, HUDToolsAgentPictureTests.swift | HUDTools+Picture.swift | The metadata and refusals are portable; pixels are not (they use stub closures). |
| PanelLayout/PanelPictureTests.swift | PanelPicture.swift (IdentityMask, scale, scrolls) | Yes. |
| MCP/MCPToolsTests.swift | MCPTools.swift (definitions and schemas) | Yes. A JSON schema snapshot. |
| MCP/MCPServerTests.swift, MCPStatelessTests.swift, MCPServerProbe.swift | MCPServer.swift, MCPProtocol.swift | Yes. JSON-RPC line transcripts. |
| MCP/MCPStdioTests.swift | MCPStdio.swift, MCPLineBuffer | Partly. Bound to pipes and processes, but portable as stdin/stdout transcripts. |
| MCP/MCPAppChannelTests.swift, MCPToolsOffTests.swift | MCPAppChannel.swift, MCPAppClient.swift, MCPToolCall.swift | Socket- and lock-bound. The message encoding is portable. |
| MCP/MCPClientTests.swift | MCPClient.swift, MCPTOMLTables.swift, MCPClientCommand.swift | The configuration parsing and arguments are portable; finding executables and running commands are bound to the filesystem and processes. |
| — | AgentHUDOpenApp/main.swift | No tests. |

Since 2026-10-21, `presentation/tool_tests.rs` covers the draft contract of `presentation/tools.rs` (reduction, the shared-layout choice, refusals, `from`). No Next test was found for `picture.rs`, `mcp/index.ts` or `HoverPanel` pairing. The earlier audit described "Presentation fixture" checks, but no such fixtures are in the snapshot.

## Architecture notes

1. **The layout rules live outside the core.** Open keeps every rule in `AgentHUDCore`: the plan, issues, reduction, filters, holds, the store, the tool answers and app drafts. In Next they are split across:
   - **the Tauri shell crate** (not `crates/core`): `presentation.rs` and `presentation/tools.rs` hold normalisation, the drafts, holds, revision, Save and Discard, the `get_hud` listing and the style mapping;
   - **TypeScript UI**:
     - `HoverPanel.tsx`: half pairing and the 460-pt threshold, and which blocks are left out;
     - `QuotaSection.tsx`: widget, measure, columns and period defaults, and the balance measures;
     - `SessionsSection.tsx`: the session filter, and a fallback sort that differs from the core's;
     - `TokenChartSection.tsx`: period → bucket size;
     - `use-hud-config.ts`: the layout → config projection and switch mapping, plus a second legacy migrator;
     - `DisplayPane.tsx`: Reset and Revert held in React refs;
     - `App.tsx:518`: the draft-bar `unsaved` rule, which disagrees with `tools.rs:325`;
     - `HUDScene.tsx`: hold handling;
     - `HUDPicture.tsx`: masking and picture size.

   None of this is reachable by a shared fixture suite.
2. **One contract, four definitions.** The same layout contract is defined separately in:
   - `normalize_layout` (Rust shell);
   - the legacy `layout()` and `carry_layout` (platform crate);
   - `pickSettings`, `getScreenHUDConfig` and `PanelLayout` (TypeScript protocol and UI);
   - the zod schema (MCP).

   They already disagree. Session states, for example, are:
   - `[running,waiting,idle,unverified]` in zod;
   - the same plus `ended` in `normalize_layout`;
   - the same plus `ended` = idle in the core query;
   - `[waiting,running,ended]` in Open.

   The session-period vocabulary and `tokenTotal` handling also differ. A single core-owned type with serde, exported to TypeScript, would remove the drift.
3. **Shared migration logic sits in an OS-specific file.** `platform/src/os/macos/legacy_preferences.rs` contains layout migration (`carry_layout`, block-key mapping, hidden-account carry) inside the macOS module. Next imports Open's preferences only on macOS, so this is acceptable, but it is business logic in an OS adapter.
4. **The shell aborts on a bad preferences file.** `Presentation::new` returns an error on a malformed file, and setup aborts (lib.rs:728-733, `expect("Agent HUD could not start")`). Open reads each part forgivingly.
5. **Concurrency.** No parallel provider, account or log reads were found in this chunk:
   - The local-tools server accepts one request at a time (unix.rs:86-125).
   - Pictures are one at a time (picture.rs:64-68).
   - Each `get_hud`, `draft_hud`, `discard_hud_draft` and `finish_hud_edit` call does run a ledger `statistics` query under the backend lock to fill `offeredModels` (desk-tools.rs `token_catalog`). That is a database read on every tool call, which Open never does ("nothing is read for it"). Since 2026-10-21 it covers the last 30 days and also checks a proposal's token agents and models (row 26).

   Without a single-instance guard (row 92), a second app copy would run its own collector beside the first, which breaks the serial-collection rule across processes.
6. **Cross-launch revision guard.** Open adds a random launch offset so a revision read before a relaunch can never match after it (HUDTools.swift:41-43). Next resets to 0 at every launch.
7. **Docs overstate parity.** `capabilities.json` and the earlier audit describe the MCP and panel-tools contract as fully migrated ("full panel widget/filter/style/app draft contract"). The missing and differing rows above contradict that.
