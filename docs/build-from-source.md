# Build from source

**Requirements:** Node.js 24+, pnpm, a current Rust toolchain (1.90+), and [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```sh
git clone https://github.com/jazzenchen/agent-hud-open.git
cd agent-hud-open
pnpm install
pnpm run check
pnpm run build:macos -- --bundle
```

Use `build:windows` or `build:linux` on those systems; each command builds only for the system it runs on. Tauri writes the app under `target/release/bundle`. To try the app without any agents, start it with `--demo`, which opens it on sample data.

**A patched Tao.** On macOS the HUD has to be a panel that never takes focus from the app you are using, and upstream Tao cannot create one yet ([tao#414](https://github.com/tauri-apps/tao/issues/414)). Until it can, the workspace takes `tao` from [a public fork](https://github.com/jazzenchen/tao/tree/agent-hud/nonactivating-panel) with one small commit on top of 0.37.1, pinned by revision in `Cargo.toml`; Cargo fetches it from GitHub on the first build. What the commit changes, and how to move it to a new Tao release: [Tao patch](tao-panel-patch.md).

**Preview.** This build is checked against the native Swift app, [agent-hud-native-open](https://github.com/jazzenchen/agent-hud-native-open) (formerly agent-hud-open), behavior by behavior, in the [parity ledger](parity/README.md). It does not install client hooks yet, so approvals, questions and most replies stay in each agent's own prompt.

<details>
<summary><strong>Development</strong></summary>

| Command | Purpose |
| --- | --- |
| `pnpm run check` | Build the interface and MCP server, check the platform boundary and the parity ledger, type-check Rust |
| `cargo test --workspace` | Run the Rust tests |
| `pnpm run dev` | Run the interface in a browser at `http://127.0.0.1:1420` on live data (run `pnpm run engine:build` first) |
| `pnpm run build:macos` / `build:windows` / `build:linux` | Build the app for the system you are on |
| `pnpm --silent run mcp` | Run the MCP server over stdio |

The workspace layout, the MCP client configuration and the other commands are in [development](development.md). Code goes where [Contributing](../CONTRIBUTING.md) says: core computes, the interface displays, and each system's code stays in its own files.

</details>
