# Agent HUD Open

A native macOS utility for agent activity, remaining usage, and local usage statistics.

Requires macOS 14 or later and a Swift 6 toolchain. The project provides source code and local build tools. Prebuilt applications are not distributed here.

## Packages

- `AgentHUDSupport`: structured JSON, stable record identities, and date encoding.
- `AgentHUDCore`: agent data sources, usage models, local history, and display calculations.

```sh
swift test
```

## Data sources

Claude Code, Codex Desktop / CLI, DeepSeek Harness, Antigravity, Cursor, Grok CLI, OpenCode, Kimi, GLM, and Pi are supported. Available activity, quota, and balance information depends on the client and account.

The application uses installed agent clients and, where needed, their configured API keys or tokens to query the corresponding provider. It requires no Agent HUD account. See [data access](docs/data-access.md) for exact boundaries.

## Development

See [the roadmap](docs/roadmap.md) for planned capabilities and their acceptance criteria.

## License

[Apache-2.0](LICENSE).
