# UI source provenance

- React, React DOM, Vite and the Tauri API/CLI are resolved from the npm `latest` stable tags and pinned by the lockfile.
- shadcn/ui: current components generated with CLI `4.22.1` from https://ui.shadcn.com. MIT. Shared components live in `src/packages/ui/src/ui`.
- MCP: official `@modelcontextprotocol/server` / `client` `2.3.1`, current stable v2 line for the `2026-07-28` specification. https://ts.sdk.modelcontextprotocol.io/v2/.

Registry-generated utility imports are adapted to `@agent-hud/ui/lib/utils` so the UI package does not import app-private modules. Shader animation is disabled with reduced motion or when the document is hidden.

- Original Agent HUD UI layouts, geometry, hover behavior and bot logic are ports from `agent-hud-native-open`, licensed under Apache-2.0. The license is included at the repository root in [LICENSE](../LICENSE). Native rendering expressions were translated to React, Canvas, SVG and Rust.
- Original provider PNGs and the Clawd sprites are retained under src/packages/hud/src/assets with original notices; the Settings wallpaper is in src/packages/ui/src/assets and the application icon in src/packages/app/src/assets. Clawd sprites retain their separate Anthropic copyright; see src/packages/hud/src/assets/README.md and THIRD_PARTY_NOTICES.txt.

shadcn visual overrides live in src/packages/ui/src/global.css; registry component behavior is unchanged. HUD geometry and timelines are product implementation code. The HUD rim's WGSL shader implements the original six glow styles and six effects and is drawn with WebGPU directly (`src/packages/hud/src/glow-gpu.ts`); it keeps a static fallback when WebGPU is unavailable.

## Agents and providers

Borrowed implementation (MIT): the Qoder and WorkBuddy web quota requests, the
WorkBuddy package codes and the China-time cycle ends follow CodexBar's
providers (github.com/steipete/CodexBar, commit 897bff3,
`Sources/CodexBarCore/Resources/Plugins/{qoder.js,workbuddy.ts}`), whose
codes come from the WorkBuddy 5.6.2 client. Agent HUD signs in in its own
window instead of importing browser cookies.

TRAE CN's credit requests and fields (the `GetUserToken` exchange, the
entitlement list, `code` 1001 as signed out, the plan from `display_desc`)
follow CodexBar's Trae CN provider (MIT,
`Sources/CodexBarCore/Resources/Plugins/traecn.js`, `docs/traecn.md`). TRAE's
hook events, their settings files and the two builds' bundle identifiers
were checked against CodeIsland (github.com/wxtsky/CodeIsland, MIT,
`ConfigInstaller.swift`, `TerminalActivator.swift`) and TRAE's own hook
reference.

- Cline, Roo Code and Goose: file layouts, field names and cache rules are taken from their own Apache-2.0 sources (Cline `7a7878c` and `v3.89.2`, Roo Code `b867ec9`, Goose `v1.57.0`); no code is copied. tokscale (MIT, `b26c433`) confirmed the Cline and Roo Code cache rules, agentsview (MIT) the Goose ledger, and CodexBar's ClinePass reader (MIT, `897bff3`) the request and answer of `src/crates/agents/src/cline/account.rs`.
- `cline.png`, `roo-code.png` and `goose.png` in src/packages/hud/src/assets are the `light/` marks of `@lobehub/icons-static-png` 2.6.0 (MIT, the notice in `LobeIcons-LICENSE.txt`), unchanged.
- Nous Portal credits (`src/crates/agents/src/hermes/account.rs`): the auth file shapes, the never-refresh rule and the trusted-portal rule follow CodexBar's Nous reader (`NousSettingsReader.swift`, `nous.ts`, commit 897bff3), MIT, Copyright (c) 2026 Peter Steinberger; re-implemented in Rust and checked against Hermes Agent 0.19 (MIT). Hermes's hook payloads and `config.yaml` layout, and Kimi Code's log records, were read from those clients themselves; no code was taken from them.
- Reading Claude Code's own usage record (`cachedUsageUtilization` in `.claude.json`) only for the account the same file names, as of when Claude Code fetched it, follows QuotaView (https://github.com/duoasa/quotaview, `ClaudeCodeAccountProfile.decode`), MIT. The Rust reader is `src/crates/agents/src/claude/usage_record.rs`.
- Devin Desktop: what runs Devin Local (`devin acp`, the bundled CLI), its session store and its ACP usage events were checked against Devin Desktop 3.11.238's own package and Devin CLI 3000.11.234, read without installing or running them. QuotaView (MIT, github.com/duoasa/quotaview) and tokscale (MIT, github.com/junhoyeo/tokscale) pointed to these facts; no code was copied from either.

## Client formats

Facts about other clients' files, and the readers built on them, come from these MIT-licensed projects; no code is copied.

- agentsview (kenn-io/agentsview, commit 3c4ccb4, MIT): Qoder's transcript folders, its sub-agent layout and the `<session>-session.json` beside a log.
- Token Monitor (Javis603/token-monitor, commit 6c78d38, MIT) and its tokscale fork (Javis603/tokscale, commit f4abd58, MIT): Qoder CN's older `local.db` store, and Qoder's input holding its cache reads, which Qoder CLI's own package (`@qoder-ai/qodercli` 1.1.82, Apache-2.0) confirms with its folders and hook payload.
- tokscale (junhoyeo/tokscale, commit b26c433, MIT): Copilot CLI's `session-store.db` layout, its input holding the cache's reads and writes, and a repeated `session.shutdown` counting once.
- AgentBar (michalstrnadel/AgentBar, commit 97f4cf4, MIT): the payload of Copilot CLI's `permissionRequest` hook, as logged from a real CLI.
- Kilo Code's and MiMo Code's logos are unchanged copies of `light/kilocode.png` and `light/xiaomimimo.png` from `@lobehub/icons-static-png` 2.6.0 (MIT; LobeHub's notice is in src/packages/hud/src/assets/LobeIcons-LICENSE.txt). LobeHub has no mark for Oh My Pi.
- Kilo Code, MiMo Code and Oh My Pi storage, plugin and extension facts come from their own MIT sources (Kilo-Org/kilocode 080ca6c, XiaomiMiMo/MiMo-Code 6b66e8c, can1357/oh-my-pi 33ea58a). Tokscale (MIT, junhoyeo/tokscale b26c433) informed guarding OpenCode-format rows with `json_valid` and leaving out Kilo Code's migrated tasks.

## Factory Droid

Factory Droid's reader, hooks and limits (`src/crates/agents/src/droid/`) take their facts from Droid 1.3.0's own code (the `@factory/cli` npm package, read, never run) and docs.factory.com, and borrow these MIT-licensed readers' rules:

- tokscale (junhoyeo/tokscale, `crates/tokscale-core/src/sessions/droid.rs`, b26c433), ccusage (ccusage/ccusage, `rust/adapters/droid`, f674ef6) and codeburn (getagentseal/codeburn, `src/providers/droid.ts`, 3faea16): the session files, the `tokenUsage` fields and the model a router or custom session names.
- CodexBar (steipete/CodexBar, `Sources/CodexBarCore/Providers/Factory/`, 897bff3): `FACTORY_API_KEY` and `~/.factory/.env`, the `billing/limits` windows and their resets, a window whose end passed read as reset, and the extra usage balance.

Copyright (c) their respective authors; each under the MIT license.
