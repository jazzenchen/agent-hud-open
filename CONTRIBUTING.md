# Contributing

This repository ports Agent HUD's native Swift app, [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open) (formerly agent-hud-open), to Rust, Tauri and React so it runs on macOS, Windows and Linux with the same behavior. In these documents, **the Swift app** means that app.

## The reference

The Swift app's behavior at commit `014524c` is the specification. [The parity ledger](docs/parity/README.md) lists, behavior by behavior, what matches and what does not. A change that closes a ledger row should say which row, and update it.

This app may differ from the Swift app only when the difference is deliberate (see the ledger's list) or clearly better. Write the reason in the ledger row.

## Where code goes

| Layer | Path | Holds |
| --- | --- | --- |
| Agents | `src/crates/agents` | What Agent HUD knows about each client, one folder per client (`src/<id>/`): its descriptor, its transcript parser, its account reader, its hook formats and its per-OS facts as data. The shared transcript model (`Log`, `LiveLog`, `Usage`) is in `src/transcript/`; account readers reach the machine only through `Host` (`src/account.rs`). No OS conditions; it reads only the logs and databases the engine names. |
| Core | `src/crates/core` | Reading logs incrementally, token accounting, sessions, quota math, pricing, the SQLite ledger: what core does with what the agents' parsers read. `src/clients/` holds the client-specific work that still touches core's own state. No OS calls, no UI. |
| Platform | `src/crates/platform` | Paths, discovery, the account scheduler and the `Host` the agents' account readers use (`accounts/host.rs`), hooks, IPC, and each client's native channel in `src/clients/<id>/` (the descriptor's `channel` says where it runs). OS-specific parts live in its `os/` and host files. |
| Runtime | `src/crates/runtime` | The one owner that composes core and platform. |
| HUD rules | `src/crates/hud` | What the HUD shows: the panel's saved layout and the rules an agent's draft must meet, Bot directions, vendor rows. No windows, no OS calls. |
| Host | `src/crates/host` | The Tauri host as a library: windows, tray, HUD windows and input, the menu bar, the extension interface (`ProductExtension`). OS-specific parts live in its host files. |
| Desktop | `src/apps/desktop` | The app: `src-tauri` is the binary, its Tauri configuration, icons and capabilities; `src/main.tsx` mounts `@agent-hud/app`. |
| App | `src/packages/app` (`@agent-hud/app`) | The React windows: Settings, Statistics, Sessions and the HUD surface. `@agent-hud/app/extension` is what a product adds pages through. |
| HUD | `src/packages/hud` (`@agent-hud/hud`) | The HUD scene, hover panel, Bots, alerts and permission cards, with the agents' artwork. |
| UI | `src/packages/ui` (`@agent-hud/ui`) | Components, the theme, number and date formatting. |
| Protocol | `src/packages/protocol` (`@agent-hud/protocol`) | The snapshot and request types the front end reads. |
| Vite config | `src/packages/vite-config` (`@agent-hud/vite-config`) | The Vite configuration every desktop front end shares. |

Front-end packages import each other only by name, through what a package lists in its `exports`, and only packages they list in their own `package.json`; relative paths stay inside a package. `pnpm run check:packages` refuses anything else. Shared dependency versions live once in the `catalog` of `pnpm-workspace.yaml`.

Code that names a client belongs in its folder under `src/crates/agents/src/`, as the contract in [docs/agent-contract.md](docs/agent-contract.md) describes. `pnpm run check:clients` reports what still names clients elsewhere while that move is under way.

Two rules matter most:

- **Core computes, the UI displays.** Numbers, levels, labels and text that Open produces in Core belong in Rust, not in TypeScript.
- **Collection reads are strictly serial.** Log reads and account readings never run at the same time. Account readings hold the collection gate (`CollectionGate`), and so does every collection pass. Do not add threads, task groups or parallel requests that read providers.

## Working on one operating system

Shared code never names an operating system. Each OS-dependent capability has a selector and one file per host; [platform architecture](docs/platform-architecture.md) maps them all. To improve Windows, for example, change the `windows` files only. A stub whose comment says "not implemented" is open work.

Every text file is checked out with LF line endings on every OS (`.gitattributes`), so the format checks agree on Windows. A Windows clone made before that file still has CRLF files on disk; with no uncommitted work, `git rm --cached -r -q . && git reset --hard` checks them out again with LF.

`pnpm run check:platform` refuses OS conditions in shared code.

## Before sending a change

```sh
pnpm run format
pnpm run check
pnpm run lint
cargo test --workspace
```

`pnpm run format` runs rustfmt and Prettier with the repository's settings, and CI refuses unformatted code. `pnpm run lint` is clippy with warnings as errors, as CI runs it on each operating system. Editors that read `.editorconfig`, `rustfmt.toml` and `.prettierrc.json` format the same way on save.

Windows compilation and adapter tests run on GitHub's native Windows runner.
Use the **Windows desktop** workflow and download its installer for real-machine
acceptance; see [platform builds](docs/platform-builds.md). There is no requirement
to install a Windows cross compiler on macOS. State which checks ran in CI and
which behaviors were actually tested on Windows.

## Tao

The workspace takes `tao` from a fork with one macOS commit, pinned by rev. [The patch notes](docs/tao-panel-patch.md) explain why and how to upgrade it.
