# Agent HUD Next MCP

The service uses the current official TypeScript MCP SDK v2 (`@modelcontextprotocol/server`) and attaches to the running native application through `@agent-hud/engine-client` and the Rust engine’s `--app-channel` relay. The SDK owns the MCP transport, protocol compatibility and input validation. No transcript parsing is duplicated in TypeScript.

Build the Rust engine from the monorepo root first:

```sh
npm run engine:build
npm --silent run mcp
```

Use a direct executable in a client configuration so npm's status messages do not enter protocol stdout:

```json
{
  "mcpServers": {
    "agent-hud-next": {
      "command": "/absolute/path/to/agent-hud-next/node_modules/.bin/tsx",
      "args": ["/absolute/path/to/agent-hud-next/src/apps/mcp/src/index.ts"]
    }
  }
}
```

The default engine path is resolved relative to the package, not the client's working directory. Set `AGENTHUD_NEXT_ENGINE` to select a packaged executable.

Turn on "Let agents arrange the HUD" in Settings → Agents → Agent plugins first; it is off by default. On supported Unix hosts the app owns a user-private local socket, with no network listener. An absent/disabled app returns an error; the relay never opens the database or starts collection. Windows named-pipe support remains unavailable.

Tools: `hud_snapshot`, `hud_refresh`, `hud_session`, `hud_statistics`, `hud_capabilities`, `hud_set_preference`. Refresh returns the application’s cached facts. Statistics/session reads use the existing Runtime. Read tools do not install hooks, submit prompts or answer approvals. Write tools change only appearance/animation preferences and declare that they are not read-only. Unsupported product features remain unavailable and are listed by `hud_capabilities`.

A product built on this app can add tool groups of its own. At start the relay asks the app for them (`host_tools` over the same socket) and lists the tools of the groups that are on; a session started while a group was off gets it from its next session. A call goes to the app as `host_tool` with its name, its arguments as given and `caller`, the agent session that called as its client names it: Codex puts the thread and turn in each call's `_meta` (`threadId`, `x-codex-turn-metadata.turn_id`), Antigravity its conversation (`antigravity.google/conversation_id`), and Claude Code starts the relay with `CLAUDE_CODE_SESSION_ID` in its environment. DeepSeek Harness names none and runs one relay per host for all its sessions, so for a call no client names the relay adds `parent`, the process that started it; the app finds the session from the note Agent HUD's Harness plugin in that host leaves while the call runs, and `parent` never reaches the product. No argument the agent writes takes its place. While a product group is on, the app keeps the socket open even with HUD tools off; HUD tools still answer only while their switch is on. `pnpm run test:mcp` runs the relay against a stand-in engine.

The relay registers itself with no client on its own. Settings → Agent plugins adds HUD tools to a client only when the person clicks Add, and removes them with Remove (`src/crates/host/src/mcp_clients.rs`, `src/crates/platform/src/hud_tools`). Adding does two things:

- **The server, as `agent-hud`, running `<app> --mcp`:**
  - Claude Code: `claude mcp add --scope user`.
  - Codex: `codex mcp add`.
  - Cursor: an entry under `mcpServers` in `~/.cursor/mcp.json`, which its editor and command line share.
  - DeepSeek Harness: a marked block in `$DSH_HOME/cordis.patch.yml`, the patch every profile reads.
  - Antigravity: an entry under `mcpServers` in `~/.gemini/config/mcp_config.json`, which its app and `agy` share. The desktop app ships without `agy`, so the file is edited directly.
- **The `customize-agent-hud` skill** (`src/crates/platform/src/hud_tools/customize-agent-hud.md`). It teaches the agent the draft, look and finish steps.
  - Each client gets it in a folder of its own: `$CLAUDE_CONFIG_DIR/skills` (else `~/.claude/skills`), `~/.agents/skills` for Codex, `~/.cursor/skills`, `$DSH_HOME/skills`, and `~/.gemini/config/skills` for Antigravity.
  - Cursor and DeepSeek Harness also read some of the other folders. They count a skill with the same folder or name once.
  - The copy carries `managed-by: agent-hud` in its frontmatter. A folder of that name without it is never changed or removed.
  - At launch the app brings the skill up to date in every client that lists the server.
