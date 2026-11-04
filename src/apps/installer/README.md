# agent-hud

Installs [Agent HUD](https://agenthud.app), the HUD for your coding agents, or updates it to the newest release.

```sh
npx agent-hud
```

It downloads the build for your system from [the releases](https://github.com/jazzenchen/agent-hud-open/releases), betas included, and checks it against the release's `SHA256SUMS` before installing.

| System | What it installs |
| --- | --- |
| macOS 14 or later, Apple silicon or Intel | `Agent HUD.app` in `/Applications`, after checking its Developer ID signature. Agent HUD then updates itself. |
| Windows | Runs the setup program. |
| Linux, x86_64 | `~/Applications/Agent-HUD.AppImage`. Some systems need FUSE 2 (`libfuse2`) to run AppImages. |

The Windows and Linux builds are unsigned test builds that have not been checked on real machines yet. On those systems, run `npx agent-hud` again to update.

## Options

| Option | Effect |
| --- | --- |
| `--release <version>` | Install this release instead, such as `0.6.0-beta.6` |
| `--dir <folder>` | Install into this folder on macOS or Linux, such as `~/Applications` when you can't write to `/Applications` |
| `--no-open` | Don't open Agent HUD afterwards |

Keep Agent HUD in one place. When it starts, it points the agents' hooks at its own path, so a copy in a new folder moves them.

If GitHub limits your requests, set `GITHUB_TOKEN` to any GitHub token and run it again.

On a Mac, Homebrew works too: `brew install --cask jazzenchen/tap/agent-hud`.

## License

[Apache-2.0](https://github.com/jazzenchen/agent-hud-open/blob/main/LICENSE)
