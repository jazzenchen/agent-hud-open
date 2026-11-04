---
name: customize-agent-hud
description: Rearrange, restyle or add to Agent HUD, the notch or dock HUD at the screen's edge and its hover panel, with Agent HUD's HUD tools (get_hud, draft_hud, get_hud_screenshot, discard_hud_draft, finish_hud_edit). Use it when the person asks to change, customize, tidy, reorder, hide or show anything in Agent HUD, such as quota or balance blocks, rows, accounts, widgets (bar, rings, segments, number, chart), panel sections, the sessions or token lists, panel width or spacing, controls of their own in the panel (a clock, a list, a picker, text), or the resting HUD's mode (notch, dynamic, simple), usage style, edge, position, size, glow, appearance or transparency. Also hover timing and other app settings when asked. Use it for "make the HUD smaller", "show only Codex", "add a clock", "改一下 HUD", "换成年轮/分段环", "刘海/动态/简约模式", and to explain what the HUD shows or why a change was refused. Changes are drafts that only the person can save.
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
   - `panel`: the hover panel as a tree of nodes, drafts included. Each node has an `id`, a `type` and its fields.
   - `catalog`: every control, data field, placeholder format, click action and edit step the tree can hold, with examples. It is the reference for building the tree; read it before adding controls.
   - `blocks[]`: each block's `rows[].id`, `accounts[].id` and `widgets`. Every block offers all its measures; a layout does not choose them.
   - `screens[]`: `key`, `hasNotch`, `primary`, `hud` (the current style), `held`
   - `sessions.offeredAgents`, `sessions.offeredProjects`, `tokens.offeredAgents` and `tokens.offeredModels`
   - `app`, `unsaved` and `drafts`

   The reply is large, mostly because of `sessions.rows`. If your client writes it to a file, pull out these fields (with `jq`, for example) rather than reading the whole file. `layout` is the same panel in an older spelling; you do not need it.
2. **Build the change:**
   - **Panel:** write `edit`, a list of steps that each name a node by its `id`. Change only what was asked; everything you do not name stays. `{"update": "claude", "widget": "rings"}` sets fields, `insert` adds a control, `move` reorders, `remove` deletes your own control or hides one of the panel's own parts, `replace` swaps your own control. Sending the whole tree back as `panel` works too.
   - **HUD style:** each style call replaces that display's style draft. Send every style field that should differ from what is saved, including the ones from your earlier style draft, which would otherwise be dropped. A style field you leave out keeps its saved value. `screens[].hud` shows the style with drafts included, and `store.screens` the saved one.
3. **Propose** with `draft_hud`, passing `screen`, the `revision` you read and `edit` (or `panel`), `hud` or both.
4. **Read the `note`** in the reply. It says where the draft shows and what happened.
5. **Look** with `get_hud_screenshot` when the result matters visually, then iterate.
6. **Finish** with `finish_hud_edit`, which lets the panel close. Then tell the person in plain words what changed and that it waits for them to hover the HUD and press Save. If they want your draft gone, use `discard_hud_draft` for that `screen`. It drops that scope's layout and style drafts and keeps the app draft.

## The panel tree

- **The panel's own parts:** the root `panel` (spacing, separators, width, countdown), its sections `blocks`, `tokens` and `sessions`, and inside `blocks` one `quota` or `balance` block per agent, named by its block key (`claude`, `codex`, `api:deepseek`). The blocks' order is also the order of the agents on the resting HUD. They cannot be deleted; `hidden: true` hides one. Their fields are in `catalog.own`.
- **Your own controls:** from `catalog.controls`: `row`, `column`, `grid`, `repeat`, `text`, `logo`, `bar`, `rings`, `segments`, `chart`, `divider` and `picker`. Put one in `panel` as a section of its own, or in `blocks` among the agents' blocks. A control placed there needs an `id`.
- **Side by side:** a `row` (or a `column` or `grid` inside it) that you place in `panel` can hold the sections `blocks`, `tokens` and `sessions`: `move` them into it. Sections, columns and grids in a row share its width equally; give one a `width` in points to fix it. A row of sections wants a wider panel: set `panel.width` (480–960) to 720 or more. Moving a section back into `panel` puts it under the others again.
- **Compact blocks:** an agent's block drawn as `rings` or `segments` with `"legend": false` becomes a small, about square tile: its logo and name with the account's plan (Ultra, Pro x5) at the right, then the measure picker, then one ring, the same larger size in every tile, with the most used window's figure inside in the picked measure, then when that window resets. Pointing at the ring shows each window's line; pointing at the plan shows the account and any note on it, which a dot marks. `ringSize` does not apply to a compact block. `span` from `"half"` to `"sixth"` puts up to six such blocks on a line where each keeps about 130 points: with `"sixth"`, six fit at a panel width of 960, four at 720, three at 540. Blocks share a line only with neighbours asking the same span.
- **Rings of your own:** a `rings` or `segments` control with `"legend": false` is a ring alone (`"center": "percent"` puts the share inside). In a `grid`, `colSpan` and `rowSpan` make a cell cover more, and `"size": "fill"` grows the ring to its cell, for uneven layouts.
- **Lining up:** any control takes a `width` in points, so the same column in different rows lines up (a name column of `"width": 90` in every row of a list). A row centres its children top to bottom, or tops them when it holds sections, columns or grids; `valign` overrides that.
- **Data:** text reads data through placeholders: `{window.used}`, `{session.title}`, `{now.time|clock}`. A `repeat` over `windows`, `balances`, `accounts`, `agents` or `sessions` draws its children once per item, and inside it the item is named `window`, `balance`, `account`, `agent` or `session`. `where`, `sort` and `limit` choose the items. Match windows by `agent` and `kind` (such as `claude-session`), not by account ids, so the panel works on any computer.
- **Clicks:** `onClick` opens Statistics, a session or a Settings page, sets or steps a `state` the control placed in the panel declares, or copies text. A `picker` shows a state's options.
- **Checks:** a draft that cannot be drawn is refused whole; the error lists every reason with the values that would work. Fix them all and send again.

### Words people use

| The person says | What it is |
| --- | --- |
| 年轮, concentric rings | `rings`: a block's `widget` in the panel, or `hud.simpleStyle` on the resting HUD |
| 分段环, 组合圆, split ring | `segments`, in the same two places |
| 进度条, bars | a block's `widget` `bar`, or `hud.simpleStyle` `bars` |
| 数字, numbers | a block's `widget` `number`, or `hud.simpleStyle` `numbers` |
| 用量画法 (Settings, Simple mode) | `hud.simpleStyle` |
| 刘海 / 动态 / 简约 | `hud.mode` `notch` / `dynamic` / `simple` |
| 悬停面板, the panel, 展开后 | the `panel` tree |
| 两栏, 并排, 横向布局, side by side | a `row` in `panel` holding sections or columns, with a wider `panel.width` |
| 对齐, line up | the same `width` on the matching control in each row |
| 小格子, 只有圆环, 紧凑, 方块 | a block's `widget` `rings` or `segments` with `"legend": false`, and `span` `"sixth"` |
| 错落, bento | a `grid` of your own `rings` with `colSpan`/`rowSpan` and `"size": "fill"` |
| 会话列表 / Token 图 / 额度 | the `sessions` / `tokens` sections, the quota blocks |

The same style name can mean the resting HUD or the panel. When the person talks about the logos at the screen's edge, or Simple mode, change `hud.simpleStyle` for that display. When they talk about the panel that opens on hover, change the block's `widget`. If it is unclear, ask which one.

### Examples

- "Show Claude as rings in the panel": `{"edit": [{"update": "claude", "widget": "rings"}]}`
- "简约模式换成年轮": `{"screen": "<display key>", "hud": {"mode": "simple", "simpleStyle": "rings"}}`. `simpleStyle` draws only in Simple mode, so send `mode` too unless `screens[].hud.mode` is already `simple`.
- "Hide Codex and put Cursor first": `{"edit": [{"update": "codex", "hidden": true}, {"move": "cursor", "before": "claude"}]}`
- "Put the sessions beside the agents, two columns": `{"edit": [{"update": "panel", "width": 880}, {"insert": {"id": "main", "type": "row", "children": []}, "into": "panel", "at": 0}, {"move": "blocks", "into": "main"}, {"move": "sessions", "into": "main"}, {"update": "sessions", "width": 320}]}`
- "Make Claude, Codex and Cursor small square ring blocks, side by side": `{"edit": [{"update": "claude", "widget": "rings", "legend": false, "span": "sixth"}, {"update": "codex", "widget": "rings", "legend": false, "span": "sixth"}, {"update": "cursor", "widget": "rings", "legend": false, "span": "sixth"}]}`. They must stand next to each other in `blocks`; `move` them there first if they do not. A wider `panel.width` fits more to a line.
- "A bento of rings, Claude's twice as big": `{"edit": [{"insert": {"id": "cells", "type": "grid", "columns": 4, "gap": "medium", "children": [{"type": "rings", "items": {"over": "windows", "where": {"agent": "claude"}}, "legend": false, "center": "percent", "size": "fill", "colSpan": 2, "rowSpan": 2}, {"type": "repeat", "over": "agents", "where": {"id": ["codex", "cursor", "antigravity"]}, "children": [{"type": "rings", "items": {"over": "windows", "where": {"agent": "{agent.id}"}}, "legend": false, "center": "percent", "onClick": {"open": "statistics", "tab": "tokens", "agent": "{agent.id}"}}]}]}, "into": "blocks", "at": 0}]}`
- "Add a clock above the agents": `{"edit": [{"insert": {"id": "clock", "type": "text", "text": "{now.time|clock}", "style": "title"}, "into": "blocks", "at": 0}]}`
- "A list of the windows that are nearly used up, above the token chart": `{"edit": [{"insert": {"id": "tight", "type": "column", "children": [{"type": "repeat", "over": "windows", "where": {"level": ["warning", "critical"]}, "sort": "-used", "limit": 5, "children": [{"type": "row", "align": "spread", "children": [{"type": "text", "text": "{window.agent} {window.short}"}, {"type": "text", "text": "{window.used}", "color": "level"}]}]}]}, "before": "tokens"}]}`

## draft_hud fields

**Panel:** `edit` or `panel`, as above. Every field of the panel's own parts and of your controls is in `get_hud`'s `catalog`. Two more layout fields stay outside the tree:

- `hud.agents`: which agents the resting HUD shows. Their order is not this list's: the HUD and the panel share one order, the order of the blocks inside `blocks` (Settings' agent order). To move an agent on the HUD, `move` its block. It is a layout field, so it is allowed with `"all"`, and it stays as it is unless you send it.
- The older spelling (`order`, `sections`, `blocks`, `style`, `sessions`, `tokens`) still works, but not together with `edit` or `panel`.

**HUD style (display key only), under `hud`:**

- **Every mode:**
  - `mode`: `notch` (only on a display with a notch), `dynamic` or `simple`
  - `appearance`: `dark`, `light` or `system`
  - `transparency`: 0–1
- **`dynamic` and `simple`:** `edge` (top, right, bottom or left), `position` (0–1) and `separateAgents`.
- **`dynamic` only:** `logoSize` (20–60), `logoGap` (0.1–0.6), `edgeDistance` (0–20) and `showLogos`.
- **`simple` only:** `simpleSize` (20–60), `simpleGap` (0.1–0.6), `ringWidth` (1–4), `simpleStyle` and `showNumbers`. `simpleStyle` is how each logo shows its usage: `rings` (concentric rings round it), `segments` (one ring split per window, the default), `bars` (bars beside it) or `numbers` (the most used window's percentage alone). `ringWidth` is also the bars' thickness, and `showNumbers` adds the percentage beside rings, segments or bars.
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
  - Anything else is a layout proposal, `edit`, `panel` and `hud.agents` included. It becomes the whole layout draft of that scope. For a display, any pending style draft you do not send again is dropped, so send layout and style together when you want both.
- **Sending back what is saved leaves no draft.** The note says so.
- **Edits apply in order** to the tree as it is now, drafts included; every step that cannot be done is a reason, and then nothing is changed.
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
