# Shared processing and operating-system adapters

The public Swift package is the behavioral reference for the local product. Its
Mac implementation is not evidence that another operating system exposes the
same files, credentials, executable paths or process APIs. Windows and Linux
adapters require independent implementation and native verification.

## Ownership

```text
React UI / local MCP / engine CLI
                |
      host (Tauri) ── hud (rules)
                |
             runtime
          /            \
     core               platform
 parser/ledger/     provider account protocols
 metrics/states       + selected OS adapter
                     macos | windows | linux
```

| Layer | Owns | Must not own |
| --- | --- | --- |
| `agents` | Each client's transcript format, account and hook formats, and its OS-dependent facts as data. | OS conditions, processes, sockets or credential stores. |
| `core` | Reading logs incrementally, source-event identity, deduplication, token classification, canonical sessions, SQLite ledger, quota math, pricing and projections. | AppKit/Win32 calls, process inspection, credential access, shell commands, or UI state. |
| `platform` shared provider adapters | Provider-specific account protocol parsing and normalization; independent account jobs. Shared provider formats can reuse one adapter across systems. | A second transcript parser, ledger writer or quota calculation. |
| `platform/src/os` | Host directories, application/executable lookup, process options, credential-store locations and OS-specific inspection. Only the selected host module compiles. | Inferring usage from installation evidence, guessing missing cycle metadata, or treating a Mac-only process probe as a Windows implementation. |
| `runtime` | One composition owner for discovery, filesystem invalidation, account jobs and the core. | UI polling that starts additional collectors, or platform-specific interpretation of the same usage event. |
| `hud` | The panel's saved layout and draft rules, MCP layout tools, Bot directions and vendor rows: everything every window and the menu bar must agree on. | Windows, events, OS calls or Tauri. |
| `host` (desktop native host) | Displays, pointer input, window levels/focus, tray and native rendering boundaries. AppKit, Win32 and Linux session code stays in separate host modules. | Reading logs, resolving account identities, writing another usage store, or HUD rules that do not need a window. |
| React / MCP | Render or expose the normalized facts and supported actions. | Detecting the OS to reinterpret tokens, launching provider readers, or manufacturing values for an unsupported adapter. |

`SourceRoot`, `ProviderDiscovery`, `AccountProjection` and the shared snapshot are
the current data boundaries. OS adapters locate and acquire evidence; shared
provider readers normalize their documented/source-proven format. Core then
applies the same event identity, accounting and metric rules on every platform.
No second accounting path is added for Windows.

`snapshot.connection.platformCapabilities` carries the selected adapter's
acquisition/verification status through the runtime. Local MCP's
`hud_capabilities` exposes the same facts. These describe acquisition adapters,
not acceptance of the complete native UI, animation or distribution pipeline.

## Where operating-system code lives

Shared code never branches on the operating system. Each capability that
differs by OS has one selector and one file per host, and every host file
provides the same items:

```rust
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod host;
```

A host that cannot do something yet still provides the item, as an explicit
stub whose comment says what is missing and what an adapter would use. Those
stubs are the list of open work for that operating system.

| Capability | Selector | Host files | Items |
| --- | --- | --- | --- |
| Paths, discovery, processes, files, secrets, app activation | `crates/platform/src/os/mod.rs` | `os/macos/`, `os/windows/`, `os/linux/`; `os/unix/` is shared by macOS and Linux | `home_dir`, `data_dir`, `caches_dir`, `source_roots`, `engine_candidates`, `processes`, `listening_ports`, `parent_processes`, `codex_cli_on_port`, `private_directory`, `owned_by_user`, `owner_only`, `private_file`, `read_stdin`, `hook_configuration`, `stored_secret`, `app_running`, `activate_app`, `OS` (whose facts the agents' descriptors give), `folder` and `locate` (an agent's per-OS location), capability constants |
| Ingestion path keys | `crates/core/src/log_files.rs`, `database/mod.rs` | `log_files_windows.rs`, `log_files_unix.rs`, `database/windows_paths.rs`, `database/unix_paths.rs` | Native path spelling and Windows-only legacy key migration |
| Antigravity session navigation | `crates/platform/src/navigation.rs` | `navigation/antigravity_windows.rs`, `navigation/antigravity_unix.rs` | `open` (shared DevTools transport stays in `antigravity.rs`) |
| Hook callback channel | `crates/platform/src/hooks/mod.rs` | `hooks/unix.rs`, `hooks/windows.rs` | `Server`, `send` |
| Shared Windows callback transport | `crates/platform/src/lib.rs` | `local_ipc.rs` facade over `os/windows/pipe.rs` | `Server`, `Handler`, `request` with caller-owned request/reply limits; bounded `read_stdin` |
| Codex Desktop IPC | `crates/platform/src/clients/codex/mod.rs`, `desktop_io.rs` | `desktop_io/unix.rs` (same-user socket), `desktop_io/windows.rs` (same-user named pipe); Windows observer stays in `desktop_windows.rs` | Shared `DesktopIpc` framing and owner RPC transport; `CodexDesktopObserver`, `daemon_tui_origin` are Unix-only |
| Local MCP channel | `crates/platform/src/local_tools/mod.rs` | `local_tools/unix.rs`, `local_tools/windows.rs` | `Server`, `request` |
| One running copy | `crates/platform/src/instance/mod.rs` (the lock itself is portable) | `instance/unix.rs` (socket beside the lock), `instance/windows.rs` (named pipe) | `Endpoint`, `listen`, `hand_over` |
| LAN listener and files | `crates/lan/src/os/mod.rs` | `os/unix.rs`, `os/windows.rs` | `wake_pair`, `wait`, `addresses`, `protect_directory`, `private_file` |
| LAN credential store | `crates/lan/src/credentials.rs` | `credentials/keychain_macos.rs`, `credentials/credential_windows.rs` | `SecretStore`: login Keychain / current-user Windows Credential Manager |
| LAN TLS identity | `crates/lan/src/identity.rs` | `identity/macos.rs`, `identity/windows.rs`, `identity/linux.rs` | `load` |
| Phone sharing | `crates/share/src/share/os/mod.rs` | `os/macos.rs`, `os/windows.rs`, `os/linux.rs` | `computer_name`, `secrets` (Linux has no grant store yet, so sharing stays off there) |
| HUD windows | `crates/host/src/hud/mod.rs` | `hud/macos.rs`, `hud/windows.rs`, `hud/linux.rs`; `hud/generic.rs` holds Tauri-only pieces a host may re-export | listed at the selector |
| App shell | `crates/host/src/shell/mod.rs` | `shell/macos.rs`, `shell/windows.rs`, `shell/linux.rs` | `install`, `window_opened`, `windows_changed`, `reopened`, `statistics_chrome`, `system_prefers_chinese`, `alert` (Linux prints to standard error until a dialog exists before Tauri starts) |
| System settings | `crates/host/src/system/mod.rs` | `system/macos.rs`, `system/windows.rs`, `system/linux.rs` | login item, `cli_candidates`, `private_output` |
| MCP relay | `crates/host/src/cli.rs` | `cli/unix.rs` (exec), `cli/windows.rs` (waits on the server) | `serve` |
| Picture capture | `crates/host/src/capture.rs` | `capture/macos.rs`, `capture/windows.rs`, `capture/linux.rs` | `capture` |
| Display changes | `crates/host/src/screens.rs` | `screens/macos.rs`, `screens/windows.rs`, `screens/linux.rs` | `install`, `shutdown` |

`pnpm run check:platform` (`src/tooling/check-platform-boundary.mjs`) enforces
the rule. Outside files and folders named `macos`, `windows`, `linux` or
`unix`, a platform `#[cfg]` may only select a `mod` or `use` line, and
`cfg!()` on the operating system is refused. CI runs it on every change.

Tests in shared files therefore run on every host, Windows included.
`pnpm run check:test-paths` (`src/tooling/check-test-paths.mjs`) refuses the
Mac habits that broke them there. A test path written from the Unix root
(`Path::new("/c")`, `("CLAUDE_CONFIG_DIR", "/c")`) is not absolute on Windows;
build it under `tempfile::tempdir()` or `std::env::temp_dir()`. Beside code that
accepts only the user's own folders, a plain folder belongs to Administrators on
an elevated Windows runner, and that code refuses it. This covers a test folder
made with `fs::create_dir_all` (make it with `crate::os::private_directory`) and
a `tempdir()` handed over itself (pass a path inside it, such as
`temp.path().join("data")`, so the code makes the folder). Mark a folder that
need not be private with `// plain directory`. Older lines are counted in
`test-path-literals.json`, so only new ones fail; it runs on the Mac before a
push and in CI. CI runs every crate's tests even after one fails.

To work on one operating system, change only its host files. A new capability
gets a selector, one file per host, and a row in this table. If the shared
code needs a new fact from the host, add an item to every host file rather
than a condition in shared code.

## Different hosts can acquire different evidence

- On macOS, application bundles, LaunchServices, Application Support and native
  process inspection are usable host mechanisms. The existing original-history
  locator is a macOS-only migration adapter.
- On Windows, application locations, executable extensions, app-data directories,
  process discovery and credential access need Windows implementations. A
  PowerShell wrapper, a `.cmd` launcher and a native `.exe` have different process
  lifecycles; the host adapter must handle the actual client installation.
- On Linux, XDG locations, available process tools and packaging differ. X11 and
  Wayland also have different HUD capabilities. On Wayland the host shows an edge
  HUD only as a wlr-layer-shell surface, which also drags natively, and refuses
  it on compositors without that protocol (GNOME). Native edge dragging is not
  implemented on X11.
- WSL is a distinct source environment. It must preserve source identity and
  Windows/WSL ownership when an adapter is added; scanning Windows and WSL paths
  indiscriminately would risk duplicate usage. Automatic WSL discovery is absent.

Evidence for installation, configuration, log ingestion and account readings is
separate. Finding an agent does not prove that its usage reader works. Unknown
or unsupported acquisition stays explicit; a generic home-directory fallback
does not establish native-platform parity.

## Editable scripts

Small command operations belong in platform-owned source scripts when they are
easier to inspect and maintain there: revealing a file, querying processes or
ports, and build orchestration. The Rust boundary passes arguments individually,
owns cancellation/output limits where applicable, and parses the result.

Provider RPC, authentication semantics, JSONL parsing, checkpointing, event
identity, SQLite writes, token math and session state remain shared Rust code.
Moving those into three scripts would create three implementations of one fact.

The current platform scripts are editable source inputs embedded at build time.
Editing one requires rebuilding that host. There is no arbitrary script runner
exposed to React or MCP, no downloaded script execution and no runtime hot reload.
Build orchestration lives in `src/tooling`; root manifests only name its entry
points. See [platform build commands](platform-builds.md).

## Qualification

The Mac build and real-account observations have been exercised locally. Host
module selection and separate build commands are structural support for future
ports; they are not proof of Windows/Linux builds, account coverage, focus,
mixed-DPI behavior or installer readiness. Each platform must build on its own
runner and verify its actual clients, credentials, files, process cancellation,
multi-window behavior and normal/rewrite/truncate log ingestion.

Windows LAN uses a non-exportable P-256 CNG key and the same certificate format
as macOS. Only the certificate is written to the protected local data directory;
TLS signatures stay in CNG. Adapter enumeration supplies active IPv4 endpoints,
and grants/tokens use a current-user DACL and Windows Credential Manager.
Windows-native persistence, ACL, pipe and process checks run in GitHub Actions;
LAN pairing and firewall reachability still need real-machine acceptance.
