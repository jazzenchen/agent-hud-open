# Development

An independent monorepo for migrating Agent HUD to Tauri 2, React and a portable Rust runtime. **This is an implementation in progress, not full feature parity with the existing app.** Next uses its own settings and database. It reads legacy preferences once when its own settings are absent, and never writes the original usage database. Client hooks require explicit enablement and a delegated canonical executable.

All implementation code lives under `src/`:

```text
src/
  apps/
    desktop/             React workspace + Tauri shell
    mcp/                 MCP server (stdio) for agents
  crates/
    agents/              One folder per client: descriptors, transcript parsers, account readers, hook formats
    core/                Portable ingestion, SQLite ledger, accounting, sessions and the snapshot
    runtime/             One application API composed from core and platform
    platform/            OS environment, source discovery, native actions, per-OS adapters
    host/                The Tauri host: windows, HUD panels, menu bar, collection thread, MCP tools
    hud/                 HUD presentation shared by the host: layout plans and the bot director
    lan/                 Local-network service for the iPhone app
    l10n/                Chinese and English strings
    engine/              JSON-lines CLI using that runtime
  packages/
    app/                 The windows: Settings, Statistics and sessions
    hud/                 The HUD
    ui/                  Components and theme
    protocol/            Shared snapshot types and capability manifest
    engine-client/       Node transport used by the development UI and MCP
    vite-config/         Shared Vite configuration
  fixtures/              Parity fixtures shared with the Swift app
  tooling/               Build entry points and repository checks
```

The root holds workspace manifests, lockfiles, documentation and build configuration. Rust `core` does not depend on Tauri, Swift, React or system frameworks. OS-specific operations belong to `platform`; desktop and MCP do not implement their own transcript readers or state machines. The desktop embeds one `runtime`. MCP’s engine relay attaches to that running application’s local channel and never starts another collector; the browser development adapter uses a standalone engine.

Every native app and standalone engine takes the data directory's exclusive
lock before opening its database. A build that owns client hooks also takes
their shared lock; owning hooks never replaces the data lock. A second native
launch of the same executable brings the running app forward. An occupied or
unavailable lock prevents another collector from starting. To use browser
development while the native app runs, give the development engine its own
`AGENTHUD_NEXT_DATA_DIR`. The MCP app-channel relay takes neither lock and
creates no collector.

## Run the workspace

Use Node.js 24+ and a current stable Rust toolchain. Dependency versions are pinned in `pnpm-lock.yaml` and `Cargo.lock`.

```sh
pnpm install
pnpm run engine:build
pnpm run dev
```

Open `http://127.0.0.1:1420`. Browser and native app startup both use **Live** data by default. The browser invokes the Rust CLI; the native app embeds the same runtime. A failed live connection stays an error; sample data is never substituted. `file://.../index.html` cannot run a Vite application.

Demo is enabled only by the native process argument `--demo`. Without that argument, a saved browser/demo preference has no effect. There is no UI switch into Demo.

```sh
pnpm run build             # React + MCP type/build checks
pnpm run desktop:build     # Compile the Tauri executable; does not launch it
pnpm --silent run mcp      # MCP stdio service; stdout is protocol-only
```

`pnpm run build:macos`, `pnpm run build:windows` and `pnpm run build:linux` build
on the matching host. They select the same shared runtime and that host's native
adapters, and reject a different host before starting compilation. See
[platform build commands](platform-builds.md); Windows/Linux execution and
provider acquisition are not yet qualified.

For MCP clients, launch the `tsx` binary with the absolute path to `src/apps/mcp/src/index.ts`, or use the configuration in [the MCP app](../src/apps/mcp/README.md). `AGENTHUD_NEXT_ENGINE` can point to an installed engine executable; the local default is `target/debug/agent-hud-engine`.

Settings, Statistics and the floating HUD run as separate windows of the single running application. Preview builds keep client-hook installation disabled unless a delegated canonical executable path is provided, preserving existing client hook configurations.

## Implemented

- Source-based Apple-style Settings, token statistics, session list/details, and a separate transparent HUD. The three HUD modes share original dimensions and all original provider artwork. The full procedural and Clawd Bot state/clip/renderer systems and mark director are ported; authoritative session facts drive their work, attention and event states.
- Official shadcn component sources remain untouched; the shared `global.css` owns their Apple-style theme. React Bits handles content entry and Shaders renders the configurable HUD glow with a static fallback.
- All desktop platforms use the same Rust core. No Swift dependency or native Apple data service is launched.
- Claude Code and Codex local JSONL ingestion, streamed/cumulative token deduplication, incremental reads, bounded processing and SQLite checkpoints.
- The original 22-entry provider catalog, with independent installed/configured/usage evidence. Incremental JSONL, zstd, native SQLite, protobuf and local-cache readers are tracked independently of catalog detection.
- Real account readers follow the original client protocols: Claude Code `get_usage`, Codex `app-server` account/rate-limit reads, Cursor's existing account database and usage endpoint, DeepSeek's configured API balance, and an already-running Antigravity local service. Pi's existing ChatGPT OAuth account is included when usable. These background jobs publish actual quota windows, plans and balances; no prompt, login or client hook installation is performed. See [account ownership](accounts.md).
- Client hook formats, Unix request delivery/withdrawal/timeouts, Codex Desktop approvals/questions and HUD question/plan forms are implemented. Notifications reproject committed facts immediately without a transcript scan. Preview hook installation is still disabled without a delegated fixed executable; no current client configuration was changed. See [client hooks](client-hooks.md).
- Claude/Codex source turn/activity reducers expose actual session phases, tool calls/results, retry/error outcomes, explicit parent IDs and read-only question facts. Source timestamps, terminal guards and the original 120s/1800s freshness rules apply. Version 4 checkpoints add complete model-attributed meter events; old checkpoints require a verified history import or bounded replay.
- SQLite stores quota samples, canonical call events and model buckets separately from the bounded UI turn list. Quota, Burn rate, Reply rate, Consumed and Cost use those facts. Legacy samples require exact account identity; unknown cycles, prices or incomplete indexing stay explicit. See [the data boundary](accounts.md).
- Exact provider IDs, local session metadata, usage entries, activity buckets and persistent Next preferences.
- Tauri 2 window and tray lifecycle; title-free Overlay windows with app-owned drag regions, per-screen native HUD windows, input regions and edge placement; platform-specific transcript reveal actions.
- macOS physical-notch measurements and stable screen UUIDs. Display preferences, panel content switches/order/width and comfortable/normal/compact spacing are independent by screen. Disconnected windows are retired, while their saved settings remain available after reconnecting.
- One application-owned Rust collector publishes snapshots to all windows. Closing or hiding Settings does not stop collection or start another collector. Cached bootstrap reads and manual requests run off the UI thread.
- Native cursor samples drive hover enter/leave independently of WebView mouse delivery. Every HUD surface is solid, with a per-screen transparency; there is no glass material. The menu bar uses the original notch silhouette, highest measured quota and native actions; its data and visibility follow the main store.
- MCP attaches to the running native app when Settings → Agent plugins → "Let agents arrange the HUD" is on (off by default). Snapshot/refresh reads reuse its cache; statistics and sessions query its existing ledger. The original panel/style/app draft tools, explicit user Save/Discard, masked native pictures and client setup are included. A disabled or absent app returns an error without starting another collector.
- Agent HUD's Claude Code plugin (`crates/platform/src/claude_plugin`, host `crates/host/src/claude_plugin.rs`) is installed and removed only from Settings → Agent plugins, with Claude Code's own `claude plugin install`/`uninstall`. Each session that loaded it checks in to a shared folder every 5 seconds (20-second reachability); the HUD's answer to a Claude desktop question is left there as the user's next message and submitted once the session is idle. "Answer questions" on the plugin's row (`settings.claudeAnswers`, on by default) decides whether those questions are answerable on the HUD. A product build may name the folder (`ProductExtension::claude_plugin_paths`) and add its own switches to the row.
- Agent HUD's DeepSeek Harness plugin (`crates/platform/src/deepseek_plugin`, host `crates/host/src/deepseek_plugin.rs`) is installed and removed only from Settings → Agent plugins: one module (`agent-hud.mjs`, only Node's own modules and Harness's `ctx`) and one marked row in `$DSH_HOME/cordis.patch.yml` that loads it, which every Harness host picks up live, the Desktop app's included. The row's id and the file's name carry the module's version, since Harness keeps running the old module under an id it knows; at launch an installed plugin whose module changed is written again. It shares a folder with the app:
  - Every open session checks in every 5 seconds (as the Claude Code plugin does), and words left for it are queued as its next message through Harness's session controller.
  - While a session calls one of Agent HUD's MCP tools, the plugin leaves a note of the call. Harness runs one relay per host, so the relay sends its parent process with a call no client names, and the app matches the note (`host_tool`'s `caller`).
  - The questions and approvals a session waits on go to the HUD beside Harness's own cards, through the waterfalls `user-questions/request` and `approval/request`. Whichever is answered first wins and the other card goes away. A host with no window can be answered on the HUD while the app runs.
  - Every listener, timer and promise in the module catches its own errors: one throw out of a timer stops the whole Harness host, and the Desktop app loads the module again at every launch.
- A product build that finds a newer version tells the host (`HostContext::update`, an `UpdateNotice` with the version and a Settings page id; None takes it away). As Open's updater showed it, the menu bar's menu gets New Version… between Settings and Quit, and every HUD panel's footer an arrow beside Statistics in the system blue, titled "Update to <version>", which grows in (fades in with reduced motion). Either opens Settings on that page (`show_settings_page`; a product page that replaces it answers). HUD pages get the notice as `hud:update` and read it when they start (`hud_update_notice`); Settings takes a pending page the same way (`settings:page`, `take_settings_page`).
- A product build may ask every HUD to show a message flying between two agents' marks (`HostContext::fly`, sent to the windows as `hud:flight`). The layer (`packages/hud/src/scene/FlightLayer.tsx`, routes and keyframes in `flight-plan.ts`) exists only while a flight is under way and animates with Web Animations.

## Still to migrate

See [the parity ledger](parity/README.md) for a behavior-by-behavior comparison with agent-hud-native-open, the native Swift app. Remaining gaps include full visual/interaction comparison, rich conversation rendering, some native-client observers, API model-row controls, native Windows/Linux qualification and signed release configuration. A compiled adapter or passing synthetic fixture does not prove live endpoint or OS parity.

Transcript state follows the events a client actually wrote; without usable evidence it stays **unverified**, rather than being guessed from file modification times. Unknown costs and quotas are unavailable. Initial indexing reports partial totals until the pending files are read. Usage entries are not a promise of full conversation turns or fork-aware accounting parity. Windows and Linux source paths are implemented but still need verification on those operating systems; WSL sources are not automatically discovered.

## UI sources

The front end is four packages: `@agent-hud/ui` (components and theme), `@agent-hud/hud` (the HUD), `@agent-hud/app` (the windows) and `@agent-hud/protocol` (types); `src/apps/desktop` only mounts the app. shadcn/ui components are generated from the current registry into `src/packages/ui` (run the shadcn CLI there). The HUD rim is one WGSL shader drawn with WebGPU directly (`glow-gpu.ts`), with no shader library: it draws only when its size or uniforms change, the soft breathe is a compositor opacity animation over that frame, and moving effects draw at the original GlowAnimator rate. It uses a still neutral rim without quota readings or with reduced motion, and a CPU still without WebGPU. See [third-party notices](third-party.md).

See [platform ownership](platform-architecture.md) for shared processing versus operating-system adapters, and [the parity ledger](parity/README.md) for remaining migration differences.
