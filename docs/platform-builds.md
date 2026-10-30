# Platform build entry points

Build orchestration lives in `src/tooling/build.mjs`; root package scripts only reference it. Application parsing, deduplication, state, and metering do not belong in build scripts. The tool invokes the existing Cargo/Tauri workspaces and does not alter their configuration or install prerequisites.

Use Node.js 24+, Bun (to compile the packaged MCP relay), the host Rust toolchain, and the dependencies installed by `pnpm install --frozen-lockfile`. Native desktop dependencies must be installed on the actual target operating system. These entry points deliberately build only for their matching host OS; they are not cross-compilation or release-qualification claims.

## Commands

Run from the repository root:

| Host | Desktop executable, release profile, no installer |
| --- | --- |
| macOS | `pnpm run build:macos` |
| Windows | `pnpm run build:windows` |
| Linux | `pnpm run build:linux` |

`pnpm run desktop:build` selects the current host and retains the same release/no-bundle default as before. All platform aliases accept the following flags after npm's `--`:

```sh
pnpm run build:macos -- --debug
pnpm run build:windows -- --component engine
pnpm run build:linux -- --bundle
pnpm run build:platform -- --platform macos --print-plan
```

Use the command matching the current host. For example, `build:windows` on macOS fails before starting pnpm, Cargo, or Tauri, with instructions to use a Windows host. An unsupported OS, component, or flag also fails explicitly. `--bundle` applies only to desktop builds and delegates host package generation to Tauri; prerequisites, signing configuration, or packaging failures remain real failures.

`--print-plan` prints the selected commands without running build tools or checking installed prerequisites. It still validates the requested platform against the current OS. It is an inspection feature, not evidence that the build would succeed.

`pnpm run engine:build` retains its debug default and output at `target/debug/agent-hud-engine` (`.exe` on Windows), which the development MCP client expects. Use `pnpm run engine:build -- --release` for the release engine. With no custom Cargo target directory, the release output is `target/release/agent-hud-engine`. `CARGO_TARGET_DIR` retains standard Cargo behavior; update `AGENTHUD_ENGINE` when choosing a different installed/output path. This tool does not silently redirect an existing MCP client.

The runner checks the Rust compiler's host OS before compiling and rejects a differing `CARGO_BUILD_TARGET`. It does not add a cross target or pass `--target`, preserving the existing Cargo output layout. Cross-architecture/universal builds are not provided by these aliases; use an appropriate build host/toolchain and qualify that artifact separately.

`pnpm run build` still performs the existing React build and MCP type checks. It does not compile a native executable. The desktop build runs the existing Tauri frontend pre-build hook and embeds the shared Rust runtime. The separate engine is needed for the standalone MCP/development transport, rather than as a second desktop collector.

## Host prerequisites and limitations

| Platform | Native build requirements | Current qualification boundary |
| --- | --- | --- |
| macOS | Xcode command-line tools/Apple SDK and the native Rust toolchain. Tauri uses the system WebView. | macOS implementation is being checked on real hardware; building alone does not certify window, animation, signing, or updater parity. |
| Windows | Visual Studio C++ Build Tools with the Windows SDK, a supported Rust toolchain, and WebView2 runtime for execution. | Native compilation and tests run in GitHub Actions; interactive Windows machine acceptance remains separate. A generated command plan is not evidence of a working build or installer. |
| Linux | The distribution's C/C++/linker tooling and Tauri 2 desktop dependencies, including WebKitGTK 4.1/GTK and required tray/SVG/packaging libraries, plus gtk-layer-shell (`libgtk-layer-shell-dev` on Debian and Ubuntu, `gtk-layer-shell-devel` on Fedora, `gtk-layer-shell` on Arch). The app links gtk-layer-shell directly, so it does not start on a system without that library, even under X11. Package names vary by distribution. | Tested by hand only on niri (Wayland, one output, scale 1). On a Wayland compositor with wlr-layer-shell the edge HUD is an overlay layer surface and drags natively; without it (GNOME) the edge HUD is refused. On X11 placement depends on the window manager and native drag is unsupported. Client hooks are not installed until a Linux package supplies a fixed executable path ([#11](https://github.com/jazzenchen/agent-hud-open/issues/11)). |

The installed Tauri CLI reports missing native dependencies. Current prerequisites are maintained in [Tauri's official platform instructions](https://v2.tauri.app/start/prerequisites/); use the instructions for the actual host rather than attempting to satisfy a Windows/Linux build from macOS.

## Windows builds on GitHub

The `Windows desktop` workflow (`.github/workflows/windows.yml`) uses an actual
`windows-latest` runner and the MSVC toolchain. It runs the package checks,
TypeScript and MCP relay tests, Rust clippy and native Rust tests, then builds
an NSIS installer.
It runs on pull requests, pushes to `main` and `codex/windows-*`, and manually
through Actions. No Windows cross-compilation on a Mac is required.

Download `agent-hud-open-windows-x86_64-unsigned` from the successful run's
**Artifacts** section. It also contains `provenance.json` identifying the source
commit and run, plus `SHA256SUMS` for the installer. The installer includes the
engine and MCP relay and installs for the current user; its WebView2 bootstrapper
downloads the runtime if needed.
These are unsigned test artifacts, not a published release. Actions keeps them
for 14 days. A green build is followed by Windows machine testing; it does not
qualify desktop focus, display, client, or network behavior.

### Windows machine acceptance

- With only Codex Desktop installed (including Microsoft Store), confirm the enabled
  Codex source appears in the HUD and its expanded panel before quota data is available.
  Missing quota displays `--`; a successful account read replaces it with the real value.
  User-disabled sources and explicitly hidden layout items remain hidden.

- Install, launch, quit and relaunch from a path containing spaces and non-ASCII
  characters; check a second launch raises the existing app.
- Read real Claude/Codex logs. In the public `agent-hud-open` app, verify that
  hook installation remains unavailable and client settings stay unchanged:
  its default host does not delegate a canonical hook-owner executable.
- In the private `agent-hud` app installed at its approved Windows path, enable
  supported client hooks from Settings, then check running/permission/completed
  states and CLI session attribution. A copied executable or a build output
  does not qualify as the hook owner.
- Configure MCP with the copyable PowerShell command, including a client
  installed through an npm `.cmd` shim; verify requests reach the running app.
- Try HUD hover, passthrough and dragging with normal and swapped mouse buttons,
  mixed display scaling, a disconnected/reconnected monitor and tray actions.
  Save different placements on two displays, restart with their enumeration order
  changed, and check placements follow each monitor's Windows device path. Mirrored
  and remote displays without a unique device path retain session-only identity.
- For the private product, pair over LAN, restart, reconnect, revoke, and confirm
  the certificate fingerprint survives restart.
- Check HUD alerts and the current-user login item. With both products installed,
  verify that enabling or disabling one product's login item leaves the other
  product's entry unchanged.

Monitor device-path identity and the private product's Remote callback pipe are
implemented; native keyboard focus handoff and all-virtual-desktop/exclusive-
fullscreen behavior remain outside the Windows capability set. The private
product's CloudKit backend remains macOS-only. See its Windows build notes for
the private installer workflow and remaining remote-control limitations.
Codex Desktop's shared Windows transport supports private explicit owner RPC;
the public background Desktop observer and CLI daemon-origin discovery remain
Unix-only. Real Desktop interoperability still needs Windows acceptance.

## Shared readers and OS-specific operations

Provider reader protocols and the parsing, usage attribution, deduplication, and metrics remain shared Rust implementation. Filesystem locations, credential access, installed-client discovery, process invocation, and native actions differ by OS and belong to the platform adapters. A shared reader is not proof that its Windows/Linux credential/path/process integration has been verified.

Platform helper scripts are editable source assets inside the platform crate and are compiled into its adapter. After editing a system helper, rebuild the component on that system; there is no runtime script override or hot reload. Build tooling itself contains only build/tool/OS orchestration and must not acquire provider parsing or accounting logic.

These commands do not launch the application, change client hooks, configure external MCP clients, copy files into `/Applications`, or publish a release.
