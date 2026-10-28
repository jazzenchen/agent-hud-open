---
name: customize-agent-hud
description: Rearrange or restyle Agent HUD, the notch or dock HUD on the Mac and its hover panel, with Agent HUD's HUD tools (get_hud, draft_hud, get_hud_screenshot, discard_hud_draft, finish_hud_edit). Use it when the person asks to change, customize, tidy, simplify, reorder, hide or show anything in Agent HUD, such as quota or balance blocks, rows, accounts, widgets (bar, ring, number, chart), panel sections, the sessions or token lists, panel width or spacing, or the resting HUD's mode (notch, dynamic, simple), edge, position, size, glow, appearance or transparency. It also covers hover timing and other app settings when the person asks for them. Use it for "make the HUD smaller", "show only Codex", "move the HUD to the left", "改一下 HUD", "刘海/动态/简约模式", and to explain what the HUD shows now or why a change was refused. Your changes are drafts, and only the person can save them in the panel.
metadata:
  managed-by: agent-hud
---

# Customize Agent HUD

Agent HUD adds its tools to this client as an MCP server named `agent-hud`. This skill uses the tools' own names. Your client may show them with the server's name in front: Claude Code, for example, calls `draft_hud` as `mcp__agent-hud__draft_hud`.

## When the tools are missing or refuse

- **No `get_hud` or `draft_hud` tool:** HUD tools have not been added to this client. Ask the person to open Agent HUD, go to **Settings → Agent plugins**, add **HUD tools** for this client and turn on **Let agents arrange the HUD**. For a client without its own card there, **Other agents → Copy settings** gives the settings to paste into its MCP configuration. Then they start a new session, because tools load when a session starts. Do not edit the client's configuration files yourself unless the person asks you to.
- **"Open Agent HUD … turn on Let agents arrange the HUD …"** or **"Local tools are disabled":** either the app is not running or its switch is off. Pass that same instruction on to the person.
- **"Application data is loading":** wait a few seconds and retry once.

## What you can change

- **Panel layout:** what the hover panel shows and in what order.
- **HUD style:** how the resting HUD looks on one display.
- **App settings:** change these only when the person explicitly asks.
- **Scope (`screen`):** `"all"` is the default. It is the shared layout, used by every display that has no layout of its own. A key from `get_hud`'s `screens[].key` targets that one display's panel and its HUD style. HUD style fields always need a display key.
- **Nothing you do is saved.** `draft_hud` shows a draft right away. The person keeps it by hovering the HUD and pressing **Save** in the panel, or throws it away with **Discard**. Drafts are lost when Agent HUD restarts, and you cannot save one yourself.
- **One exception, `hud_set_preference`:** it saves the app-wide `appearance` (dark, light or system) or `animations` (true or false) **immediately**. Use it only when the person explicitly asks for that setting.

## Workflow

1. **Read** with `get_hud` (pass `screen` to read one display). Keep these fields:
   - `revision`
   - `layout`: what that scope draws now, drafts included
   - `blocks[]`: `key`, `kind` (quota or balance), `widgets`, `measures`, `rows[].id`, `accounts[].id`
   - `screens[]`: `key`, `hasNotch`, `primary`, `hud` (the current style), `held`
   - `sessions.offeredAgents` and `sessions.offeredProjects`
   - `tokens.offeredAgents` and `tokens.offeredModels`
   - `app`, `unsaved` and `drafts`

   The reply is large, mostly because of `sessions.rows`. If your client writes it to a file, pull out these fields (with `jq`, for example) rather than reading the whole file.
2. **Build one whole proposal:**
   - **Layout:** start from the `layout` that `get_hud` returned, change only what was asked, and send all of it back. A layout field you leave out returns to its default.
   - **HUD style:** each style call replaces that display's style draft. Send every style field that should differ from what is saved, including the ones from your earlier style draft, which would otherwise be dropped. A style field you leave out keeps its saved value. `screens[].hud` shows the style with drafts included, and `store.screens` the saved one.
3. **Propose** with `draft_hud`, passing `screen`, the `revision` you read and the proposal.
4. **Read the `note`** in the reply. It says where the draft shows and what happened.
5. **Look** with `get_hud_screenshot` when the result matters visually, then iterate.
6. **Finish** with `finish_hud_edit`, which lets the panel close. Then tell the person in plain words what changed and that it waits for them to hover the HUD and press Save. If they want your draft gone, use `discard_hud_draft` for that `screen`. It drops that scope's layout and style drafts and keeps the app draft.

## draft_hud fields

**Layout:**

- `order`: block keys, top first.
- `sections`: the order of `"blocks"`, `"tokens"` and `"sessions"`. `hiddenSections` hides some of them.
- `countdown`: `false` hides the reset countdowns.
- `blocks.<key>` takes these fields:
  - `widget`: `bar`, `ring`, `number` or `chart`. Balances (`api:…`) take only `number` or `chart`.
  - `measures`: from `quota`, `burn`, `tokens`, `consumed` and `cost`. A quota `chart` takes only `tokens`. A balance `number` takes `quota`, `burn` and `cost`, and a balance `chart` takes `cost`.
  - `hidden`
  - `rows` sets the order of row ids, and `hiddenRows` hides rows.
  - `accounts` shows only the accounts listed, and `hiddenAccounts` hides accounts. Use `accounts[].id`.
  - `countdown`: quota blocks only.
  - `columns`: 2–6, for a quota block's `ring` or `number`.
  - `ringSize`: `small`, `medium` or `large`, for `ring` only.
  - `span`: `full` or `half`. Bar rows cannot be `half`.
  - `period`: `day`, `week` or `month`, for a quota `chart` or a balance.
- `style`:
  - `spacing`: `compact`, `regular` or `roomy`
  - `separators`
  - `width`: 480–720 (default 540)
- `sessions`:
  - `limit`: 1–10 (default 5)
  - `period`: `day` or `week`
  - `agents`: names from `offeredAgents`
  - `projects`: an id from `offeredProjects`, or a path inside one
- `tokens`:
  - `period`: `day`, `week` or `month`
  - `agents` and `models`: names from `tokens.offered*`
- `hud.agents`: the agents the resting HUD shows. This is a layout field, so it is allowed with `"all"`.

**HUD style (display key only), under `hud`:**

- **Every mode:**
  - `mode`: `notch` (only on a display with a notch), `dynamic` or `simple`
  - `appearance`: `dark`, `light` or `system`
  - `transparency`: 0–1
- **`dynamic` and `simple`:** `edge` (top, right, bottom or left), `position` (0–1) and `separateAgents`.
- **`dynamic` only:** `logoSize` (20–60), `logoGap` (0.1–0.6), `edgeDistance` (0–20) and `showLogos`.
- **`simple` only:** `simpleSize` (20–60), `simpleGap` (0.1–0.6), `ringWidth` (1–4), `showRings` and `showNumbers`.
- **`reset: true`:** restores the default look. Send it alone, with no other style field.
- **`glow` (not drawn in `simple`):**
  - `style`: `blur`, `dots`, `ascii`, `blocks`, `braille` or `binary`
  - `effect`: `breathe`, `flow`, `scan`, `ripple`, `shimmer` or `boot`
  - `brightness`: 0.2–1
  - `speed`: 1–12
  - `idleSpeed`: 1–24, in steps of 0.5
  - `amplitude`: 0–1, for `breathe` only
  - `range` and `blur`: 0–36, and `outwardOnly`. These work with the `blur` style only.
  - `gridPitch`: a whole number from 4 to 12
  - `gridDensity`: 0.5–1.5, in steps of 0.05
  - `gridCore` and `gridFade`: whole numbers from 0 to 8
  - The four `grid*` fields work with grid styles only.

**Versions:** `from` takes `"default"`, `"previous"` (the version saved before the last Save) or `"shared"` (a display only: it makes that display follow the shared layout). Send `from` with no layout fields.

**App (only on request), under `app`:**

- `appearance`. It is refused while `matchScreen` is on.
- `matchScreen`, `botLogos` and `showMenuBarIcon`.
- Under `hover`: `delayMs` and `collapseDelayMs` (0–1500, in steps of 50), `pressOptionToOpen` and `openAtTopEdge`.
- `language` and `writtenIn` are read-only.

`null` counts as not given.

## Rules that decide what a call does

- **What kind of call it is:**
  - Only HUD style fields: only the style changes. The layout is untouched, and the panel closes so the restyled HUD is visible.
  - Only `app`: only the app draft changes.
  - Anything else is a layout proposal, `hud.agents` included. It replaces the whole layout of that scope. For a display, any pending style draft you do not send again is dropped, so send layout and style together when you want both.
- **Sending back what is saved leaves no draft.** The note says so.
- **A refused call changes nothing.** The error starts with `Nothing was changed.` and lists every reason, with the valid keys or values. Fix all of them and send again.
- **"The layout changed since you read it":** call `get_hud` again, rebuild the proposal, then propose.
- **Displays with their own layout do not show a shared (`"all"`) draft.** The note names them. Draft for that display's key instead, or use `from: "shared"`.
- **A block, row or account that is away** (kept in the layout but showing no data now): send it back unchanged or leave it out.
- **Agent and project names must exist now.** `sessions.agents` needs agents that have sessions, `tokens.*` needs names from the chart's offered lists, and `hud.agents` needs agents the target HUD can show. The refusal lists the ones that are valid.
- **The edit hold:** a layout edit keeps that display's panel open for 2 minutes, renewed with each edit. With `"all"`, the first display is held. `finish_hud_edit` ends the hold but does not save.
- **Moving off the notch:** a HUD changed from `notch` to a dock starts at position 0 (top left). Propose another position after the person saves.
- **When style drafts are dropped:** a style draft is also lost when the person moves the HUD or changes that display in Settings. An app draft is lost when they change that setting in Settings.

## Screenshots

`get_hud_screenshot` takes `screen`, `show` (`"panel"` by default, or `"hud"`) and `agent` (optional).

- **What it shows:** the app's own panel, or one display's resting HUD, with drafts included. It renders offscreen and never captures the desktop.
- **Masking:** accounts are masked, and session titles and notices are left out.
- **Display key required:** `show: "hud"` and `agent` need one. `agent` also needs a `dynamic` or `simple` HUD with `separateAgents` on.
- **One at a time:** it renders one picture at a time and times out after 25 s.
- **Motion:** an animated glow may not show in a still picture.

## Other tools

`hud_snapshot`, `hud_refresh`, `hud_sessions`, `hud_session` and `hud_statistics` answer questions about usage and sessions without changing anything. `hud_capabilities` lists which product features are built. You do not need it for editing, because `get_hud` lists what can be set.

## With the person

- Say what you changed in plain words, and say that it is a draft waiting for them to press Save in the panel.
- Leave app settings and `hud_set_preference` alone unless the person asked for them.
- `get_hud` is not masked: it contains account names and session titles. Do not repeat them unless they are needed.
