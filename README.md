# Agent HUD Open

A native macOS utility for agent activity, remaining usage, and local usage statistics.

Requires macOS 14 or later and a Swift 6 toolchain. The project provides source code and local build tools. Prebuilt applications are not distributed here.

## Build and run

Install Xcode or the Xcode Command Line Tools with a Swift 6 toolchain, then:

```sh
git clone https://github.com/jazzenchen/agent-hud-open.git
cd agent-hud-open
make test
make run
```

`make build` creates `build/Agent HUD Open.app`. No developer account, signing identity, or provisioning profile is required. The app is signed ad-hoc for local use. Use `make demo` to preview with sample data, or `make snapshot` to render the interface to `build/snapshots`.

The app runs in the menu bar and can display an expandable notch panel with a usage glow. Settings include agent visibility, quota thresholds, appearance, language, and local startup preferences. The statistics window shows active sessions, token usage, quota trends, and available API balances. Press **⌘⌥H** to toggle the glow.

## Modules

| Product | Responsibility |
| --- | --- |
| `AgentHUDSupport` | Structured JSON, deterministic record identities, and dates |
| `AgentHUDCore` | Agent providers, usage models, local caches, and calculations |
| `AgentHUDDesktop` | Native menu bar, notch, settings, and statistics UI |
| `AgentHUDOpen` | Standalone macOS executable |

The libraries can also be consumed through Swift Package Manager. `DesktopApplication` accepts a `SettingsStore` and `UsageStore`; the host owns any additional services. See [architecture](docs/architecture.md).

## Data sources

Claude Code, Codex Desktop / CLI, DeepSeek Harness, Antigravity, Cursor, Grok CLI, OpenCode, Kimi, GLM, and Pi are supported. Available activity, quota, and balance information depends on the client and account.

The application uses installed agent clients and, where needed, their configured API keys or tokens to query the corresponding provider. It requires no Agent HUD account. See [data access](docs/data-access.md) for exact boundaries.

## Development

See [the roadmap](docs/roadmap.md) for planned capabilities and their acceptance criteria.

## License

[Apache-2.0](LICENSE). Included provider references and icon assets retain their [third-party notices](THIRD_PARTY_NOTICES.txt) and [icon license](Sources/AgentHUDDesktop/Resources/LobeIcons-LICENSE.txt).
