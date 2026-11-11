# Tao panel patch

On macOS the HUD must be a nonactivating `NSPanel` from the moment it is allocated, and upstream Tao cannot create one. The workspace therefore takes `tao` from a public fork that carries two small commits on top of the released 0.37.1: the panel, and a Windows fix for a crash (see [The Windows runner count](#the-windows-runner-count)). `tauri`, `tauri-runtime` and `tauri-runtime-wry` come unmodified from crates.io.

## Source

| | |
| --- | --- |
| Fork | https://github.com/jazzenchen/tao, branch `agent-hud/nonactivating-panel-arc-runner` (the panel commit `4a401fab`, then the Windows commit) |
| Pinned rev | `04fe4478c9d5d0febbff4d0714e10a8420a2eb1f` |
| Base | tag `tao-v0.37.1` (`37b7e8bc90a050e93be988df636f322c3ef147b6`) |
| Selected by | `[patch.crates-io]` in the root `Cargo.toml`; `Cargo.lock` records the `git+https://github.com/jazzenchen/tao?rev=…` source |
| License | Apache-2.0, upstream's `LICENSE` and `LICENSE.spdx` unchanged |
| `tao-macros` | The fork's `Cargo.toml` takes `tao-macros` (Android only) by path, so `Cargo.lock` lists it twice: from crates.io (required by `wry`) and from the fork's git source (required by `tao`). Both are 0.1.4 and Android-only; neither is compiled for desktop targets. |

The base tag's `src/` is byte-identical to the crates.io `tao-0.37.1` package (sha256 `f37f381f4e048e6cdf038b5705f8cf14ad108279d46eb968140a7b291aba9400`). Until 2026-10-20 the patched crate was vendored as `src/vendor/tao-0.37.1`; the fork's `src/` is byte-identical to that copy. The panel commit changes:

- `src/platform/macos.rs`: `WindowBuilderExtMacOS::with_nonactivating_panel` and the re-exported scope.
- `src/platform_impl/macos/mod.rs`: exports the scope.
- `src/platform_impl/macos/window.rs`: the scope, the builder attribute, class selection, panel style and flags, creation ownership, the focusable setter, and a unit test for the scope.
- `src/platform_impl/macos/window/window_class.rs` (new): the `TaoWindow` and `TaoNonactivatingPanel` classes.
- `Cargo.toml`: the `objc2-app-kit` feature `NSPanel`.

## What the patch does

The only API addition is a macOS-only, per-window creation hint: `WindowBuilderExtMacOS::with_nonactivating_panel(bool)` and the thread-local creation scope `tao::platform::macos::with_nonactivating_panel(|| ...)`. It defaults to `false`. The HUD creates its `WebviewWindowBuilder` inside the scope (`panel_builder` in `src/crates/host/src/hud/macos.rs`), so `PlatformSpecificWindowBuilderAttributes::default()` captures `nonactivating_panel: true` on that builder. Settings, Statistics, picture and other ordinary windows keep their original class. No title, transparency or focus flag is used to guess the window type.

A marked window is allocated as `TaoNonactivatingPanel`, an `NSPanel` subclass with `NonactivatingPanel` in its initial style mask, `hidesOnDeactivate` off, and `becomesKeyOnlyIfNeeded` and `floatingPanel` on. It owns its own `focusable` ivar, forwards `sendEvent:` to its actual superclass, keeps Tao's view, delegate and lifecycle, and never becomes main. Key eligibility follows the existing focusable setter. Like agent-hud-native-open's Swift `OverlayPanel`, it does not constrain its frame to the screen, and resigning key clears the focusable flag and the first responder. No `object_setClass`, delegate replacement, view reparenting or Swift process is involved.

`TaoWindow` and the panel class share `window/window_class.rs`. Two fixes come with it:

- The focusable setter writes the live object's ivar. Upstream clones the window and leaks one retained reference on every call.
- Creation adopts the `alloc`/`init` family's owned +1 with `Retained::from_raw`, following objc2 0.6.4's documented example. Upstream 0.37.1 retains it a second time.

Both classes keep Tao's close/drop path (`setReleasedWhenClosed(false)` and the owned window's asynchronous close); callback and delegate ownership is unchanged.

Superclass sends are lexical: the normal `sendEvent:` starts at `NSWindow`, the panel's at `NSPanel`, and the panel's `resignKeyWindow` at `NSPanel`. Never derive the lookup start from `object_getClass(self).superclass()`: AppKit or KVO can put a runtime subclass above the declaring Tao class, and that expression then dispatches into the same Tao implementation again.

## The Windows runner count

The second commit changes two Windows files, `src/platform_impl/windows/event_loop.rs` and `event_loop/runner.rs`: the event loop runner is shared through an `Arc` instead of an `Rc`.

Tao's Windows `EventLoopWindowTarget` holds that shared runner and derives `Clone`. `tauri-runtime-wry` keeps a window target in its `Context`, which also derives `Clone` and is declared `Send` and `Sync`, and every `tauri::AppHandle` clone or runtime call clones and drops it. The HUD asks for the pointer position and the monitors from worker threads (`hud/input.rs`, `hud/drag.rs`, `hud/displays.rs`), so with an `Rc` several threads changed one plain counter. A lost update frees the runner while it is in use or trips `Rc`'s own abort: Open #51 saw heap corruption (`0xc0000374`) followed by an illegal instruction (`0xc000001d`) after about 27 idle minutes.

Only the count crosses threads. The runner's `Cell` and `RefCell` state is still used on the event loop thread alone, as before. Upstream 0.37.2 still uses the `Rc`; keep this commit when rebasing.

## How the HUD uses it

An explicit text-input lease makes the panel key without activating Agent HUD (`begin_text_focus` in `hud/macos.rs`). Releasing the lease clears the responder and orders the panel out and straight back in, so WindowServer returns keys to the still-active underlying application. Ordinary hover and buttons do not grant the lease. A HUD drag holds key the same way, because macOS takes the cursor only from the key window. The `NSWindow` fallback for a non-panel host remains in place.

## Rejected alternatives

- A class-swizzling plugin such as tauri-nspanel replaces the live window's class with `object_setClass`. A plain `NSPanel` class lacks Tao's `focusable` ivar, so Tao's `set_focusable` then panics with "ivar not found". The HUD calls it in seven places.
- A `.patch` file plus a prepare script: with a missing `[patch]` path even `cargo metadata` fails, so fresh clones, `cargo test` and rust-analyzer break until the script runs.
- Vendoring the crate: 1.5 MB and about 120 files of every platform's code for five changed files.

## Upgrading

Status on 2026-10-20: crates.io's latest `tao` is 0.37.1. tauri-apps/tao#1368 (the creation over-retain, plus the missing `[super dealloc]` in `TaoView` and `TaoWindowDelegate`, which this patch lacks) is merged and waits for the release PR tauri-apps/tao#1357. Maintainer draft tauri-apps/tao#1387 moves `TaoWindow` to `define_class!`. Upstream has no NSPanel support (tauri-apps/tao#414, tauri-apps/tauri#13034).

- **When tao 0.37.2 ships** (tauri-apps/tao#1357, which includes tauri-apps/tao#1368): rebase onto tag `tao-v0.37.2` and drop the creation-retain hunk, since upstream makes the same `Retained::from_raw` change. A trial rebase onto upstream `dev` (`176eaef`) on 2026-10-20 conflicted in `Cargo.toml` (keep upstream's new `NSWindowScripting` next to `NSPanel`) and in three `window.rs` hunks: the imports, `create_window` (upstream now returns early with `?`; re-add the panel flags there), and the old `TaoWindow` class block (keep `mod window_class;`). Upstream also moved ObjC callbacks to `extern "C-unwind"` (tauri-apps/tao#1354); do the same in `window_class.rs`.
- **When tauri-apps/tao#1387 lands**: rewrite `window_class.rs` with `define_class!`. Upstream's `TaoWindow` then owns its ivars and its setter no longer leaks, so the panel becomes a second `define_class!` class with `NSPanel` as superclass and the same ivars. The creation hint is then the only lasting change.

Steps:

1. In a clone of the fork, fetch upstream's tags and start a new branch from the new tag, for example `agent-hud/nonactivating-panel-0.37.2`. Never force-push a branch whose rev is pinned here: older checkouts still fetch it, and GitHub may stop serving an unreachable commit.
2. Re-apply the commit, resolve as above, run `cargo test --lib` in the fork on macOS, and push the branch.
3. Set the new `rev` in the root `Cargo.toml`, run `cargo check --workspace`, and confirm that the `tao` entry in `Cargo.lock` has the new `git+https://github.com/jazzenchen/tao?rev=…` source. Cargo ignores a `[patch]` whose version no longer satisfies what `tauri-runtime-wry` requires, with only a "patch was not used" warning; check the source again after every Tauri upgrade.
4. Repeat the creation, focus, close and multi-screen checks in the real app.
