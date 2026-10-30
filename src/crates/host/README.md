# Tauri desktop host

`agent-hud-host` is the host as a library. The app binary (`src/apps/desktop/src-tauri`) only passes its Tauri context: `agent_hud_host::run_with_context(tauri::generate_context!(), None)`; a product passes its own context and a `ProductExtension`. What the HUD shows, as rules every window agrees on, is in `agent-hud-hud` (`src/crates/hud`).

The desktop host embeds the Rust `agent-hud-runtime` directly. `native_request(method, params)` dispatches to that runtime on a blocking worker, behind a single serialized data owner. There is no Swift runtime or native helper process. HUD window and input ownership are separate from the data runtime.

The `main` window is Settings (960 × 720, minimum 880 × 560). `open_statistics` lazily opens a separate `statistics` window at `index.html?window=statistics` (800 × 1000, minimum 750 × 620). Settings and Statistics can remain open simultaneously. Closing either one destroys its window, ending its page and web content process; the next open builds it again, Settings from its window configuration. With no window open the app keeps running in the menu bar. `hud_show_main` shows and focuses Settings for the HUD's Configure action, building it when it is closed; the HUD's Stats action invokes `open_statistics`.

Settings owns nothing the other windows need. A coordinator thread in the host (`coordinator.rs`) plans the HUD windows, draws the menu bar item, keeps the notch glow switch, saves a dropped HUD's place and directs the Bots, woken by collection passes, display preference edits, display changes, the system appearance and Bot timers. A page listens to the app's events only while its window is on screen (`window:activity`), and when it is back it reads what it missed: `cached_snapshot`, the display preferences, the monitors and geometry, and `hud_bot_directions`. Tauri evaluates an event's payload in every page that still has a listener for that event, whatever the target, so only an unsubscribed page receives nothing.

The menu-bar item draws the original 22 × 11 notch silhouette with its 1.5-point outline and quota-status gradient, followed by the highest observed usage percentage (or `—`). The host projects it from each collection pass (`tray.rs`): the language, the Show Menu Bar Icon setting, the glow switch, the primary screen's HUD appearance, the highest usage, the status levels and the quota, account and balance entries of the agents the primary screen's HUD draws. Native menus preserve separators, disabled headers and optional checkmarks. Their data rows open Statistics; the actions provide Show/Hide notch glow, Statistics, Refresh data, Settings and Quit, all handled natively. The glow switch lasts until the app quits, as Open's does. Settings and Quit retain their source shortcuts; on macOS ⌥⌘H toggles the glow from anywhere. New Version… appears between Settings and Quit only while the product offers a newer version (`HostContext::update`), as Open's updater added it; it opens Settings on the page the notice names. A native menu cannot color it or give it a tooltip, so the version shows in Settings.

Display reconciliation destroys disconnected HUD windows. The tray currently uses the OS menu rather than the original custom styled panel. Windows cannot show a tray title; the same usage figure is included in its tooltip. Linux title/tooltip and tray integration depend on the desktop's status notifier support. Compiling this crate never starts the GUI.

## Per-screen HUD windows

Each configured connected screen owns a transparent, borderless, non-focusable HUD window, its own geometry, interactive regions and drag state, and one lazily started pointer worker. Configuring another screen creates or updates that screen's owner; it does not move an existing screen's HUD. Multiple HUD windows can be visible simultaneously.

`hud_monitors` returns `screenId`, `windowLabel`, session-only `index`, `runtimeId`, nullable `persistentId`, name, physical `bounds`/`workArea`, `scaleFactor`, primary-display status, and a nullable measured `notch`. The macOS notch rectangle uses global logical points with the primary display's top-left at `(0, 0)`, x increasing right and y increasing down; `notchCoordinateSpace` identifies those units. The physical monitor rectangles retain Tauri's native coordinate representation, including negative origins.

On macOS, `NSScreenNumber` supplies the native display ID and the public `CGDisplayCreateUUIDFromDisplayID`/`CFUUIDCreateString` APIs supply `persistentId`. Its UUID string is also `screenId`, using the same unprefixed key as the original app's `ScreenIdentity.swift`. If a UUID is unavailable, `persistentId` remains null and `screenId` falls back to the session ID `display:<CGID>`.

On Windows, `QueryDisplayConfig` and `DisplayConfigGetDeviceInfo` match Tauri's GDI display name to the monitor device path. An unambiguous device path supplies `persistentId` as `windows-display:<normalized-device-path>`, so display enumeration order and desktop placement do not change the saved layout key. This is a Windows device identity, not a portable hardware serial: another connection or a driver change can change it. Mirrored sources, missing paths, unavailable console sessions, and duplicate device paths retain a null `persistentId` and a session key. Native tests cover matching, ambiguity, and the display query; reconnecting physical monitors still needs Windows GUI acceptance. Linux exposes `monitor:<index>` session IDs with null `persistentId`.

The native window label is `hud-` followed by the UTF-8 hex encoding of `screenId`. Its URL is `index.html?surface=hud&screen=<URL-encoded screenId>`. The HUD page reads its own screen ID from this URL and makes the document background transparent. Capabilities include the `hud-*` window pattern for event permissions.

The app plans every connected screen's HUD window itself (`hud/plans.rs`): at launch, when displays change (AppKit's screen-parameters notification on macOS; a measurement every 3 seconds on the other hosts), when display preferences change and with each new snapshot. A screen's plan is its saved mode, edge and offset (`notch` is forced to top/center), a transparent canvas wide enough for the resting strip beside the open panel and at least 940 points, as tall as the screen, and the HUD Visible setting. A window is configured only when its plan changed. Each screen's presentation preferences belong to the presentation owner; measured native geometry is transient and is not saved as presentation configuration.

Canvas dimensions are logical points and are clamped to the selected display. Offset runs left to right on horizontal edges and top to bottom on vertical edges. The canvas offset and the strip's offset together place the visible strip along the display's available travel. `notch` mode's collapsed visible rectangle comes from measured geometry, rather than treating the canvas size as a hardware notch. A configured window's frame is read back from the native window; this does not claim that a window manager honored a position that readback shows it changed.

## Measured display geometry

`geometry`, `hud:geometry` and the read-only `hud_geometry` command share this payload:

```ts
type Rect = { x: number; y: number; width: number; height: number };
interface Geometry {
  screenId: string;
  monitorIndex: number;
  scaleFactor: number;
  coordinateSpace: 'global-logical-top-left' | 'monitor-scaled-physical-top-left';
  screen: Rect;
  menuBarHeight: number;
  hasNotch: boolean;
  notch: Rect | null;
  canvas: Rect;
  notchInViewport: Rect | null;
  placementConstrained: boolean;
}
```

On macOS, `coordinateSpace` is `global-logical-top-left`: `screen`, `notch` and `canvas` share one global point plane. Public AppKit calls run on the main thread. Each `NSScreen` is matched to the selected Tauri monitor's physical bounds and scale. `safeAreaInsets.top` gives the notch height; `auxiliaryTopLeftArea` and `auxiliaryTopRightArea` give its horizontal boundaries, following the original `NotchGeometry.swift`. A display without a hardware notch has `hasNotch: false` and null notch rectangles; its menu-bar height is measured from the full and visible screen frames, with the original 22-point minimum.

The Mac adapter sets level/collection behavior first, then applies the complete requested frame with public `NSWindow.setFrame(_:display:)` in the same main-thread transaction. It orders the window forward and reads back the native content rectangle. A changed frame gets one public correction attempt and another readback; `placementConstrained` reports any remaining position or size mismatch. It does not guess a fixed menu-bar offset, change the Objective-C class, or assert that AppKit constraints were bypassed. Tao's borderless window is not a titled window; the original app's explicit `constrainFrameRect` override is not reproduced here.

On non-Mac hosts, `coordinateSpace` is `monitor-scaled-physical-top-left`: both screen and actual canvas physical coordinates are divided by the selected monitor's scale. Their differences are comparable within this payload; values from differently scaled monitors are not a uniform global logical desktop. `hasNotch` is false, both notch rectangles are null, and `menuBarHeight` is a 24-point stand-in rather than a measured platform menu bar.

`notchInViewport` is already relative to the actual canvas top-left and can position the rendered collapsed notch directly. When it is null, the frontend uses a 200-point stand-in centered in `screen`, subtracts the same-space `canvas` origin, and uses `menuBarHeight` for its height.

A new HUD may miss the first geometry event while its webview loads. Subscribe first, then query its saved native geometry; avoid letting an older query overwrite a later event:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { NativeHUDGeometry } from './lib/use-hud-geometry';

async function watchOwnGeometry(apply: (geometry: NativeHUDGeometry) => void) {
  const screenId = new URLSearchParams(location.search).get('screen');
  if (!screenId) throw new Error('Missing HUD screen ID');
  let revision = 0;
  const unlisten = await listen<NativeHUDGeometry>('hud:geometry', ({ payload }) => {
    if (payload.screenId !== screenId) return;
    revision += 1;
    apply(payload);
  });
  const before = revision;
  const current = await invoke<NativeHUDGeometry | null>('hud_geometry', { screenId });
  if (current && revision === before) apply(current);
  return unlisten;
}
```

A registered HUD caller can query only its own owner, with no arguments or with its matching `screenId`. Settings/Statistics can request a particular `screenId` or current `monitorIndex`; without a selector, they receive null. An owner that has not produced geometry also returns null. Native state is saved before the geometry event is emitted.

## Interactive regions and dragging

`ignoreCursor: true` initially passes input through the entire canvas. Each HUD reports only its current interactive surface rectangles:

```ts
const regions = interactiveElements.map((element) => {
  const { x, y, width, height } = element.getBoundingClientRect();
  return { x, y, width, height };
});
await invoke('hud_report_regions', { regions });
```

Only registered `hud-*` callers may report regions or start a drag. Native routing uses the caller's registered window label, so neither command accepts a screen ID that could redirect input to another owner. Rectangles use viewport/CSS points; report again when layout or animated geometry changes. An empty array makes the whole canvas pass-through.

After the first report, that screen's worker samples the real native cursor every 32 ms while visible, accepting input only inside its reported rectangles. Hidden-window checks slow to 500 ms, empty-region tracking waits, and application exit stops every owner's worker. Cursor-read failures fall back to pass-through. Once regions exist, configure preserves this input owner's control. This is rectangle hit testing with polling, not per-pixel input or full parity with the original native hover implementation.

Native hover is authoritative: `hud:pointer` is directed to the owner's local HUD window with `{screenId, inside, point, buttons, altKey, ctrlKey, metaKey, shiftKey}`. Points are real viewport/CSS coordinates, including negative/outside positions for Bot gaze. Outside motion notifications are limited to 12 fps using the same native worker; enter/leave transitions and failed reads are immediate, and inside movement/button/modifier changes remain precise. Outside button/modifier fields are null; a failed position read or hidden window also has a null point. macOS reads public `NSEvent.pressedMouseButtons` and `modifierFlags`; Windows queries public mouse/modifier state. Linux currently has no button/modifier backend and reports those fields as null. No AX permission, event tap, global keyboard listener or focus activation is involved. Input acceptance is applied before publishing an entering event.

Every region report samples the current cursor immediately, including a stationary cursor, publishes the current pointer and returns `{regionCount, ignoreCursor, pointerPolling, pointer}`. The RPC readback recovers a first event missed while the webview listener loads. The frontend starts native hover with an explicit outside state, serializes region reports while coalescing the latest pending geometry, and lets a newer event win over an older RPC readback. It does not depend on DOM pointerenter arriving after OS passthrough switches off. DOM hover remains available for browser previews.

The frontend detects a 300 ms hold or 6-point movement before invoking `hud_drag_start` with `{mode, edge, strip, pointer, runs: {horizontal, vertical}}`. The armed/picked-up surface shows the original dashed outline and closed-hand cursor; releasing a stationary armed hold does not become an ordinary click. `pointer` is the original mouse-down in viewport points; `strip` supplies the original grip fraction. The two runs are the actual lengths of the converted mode in its horizontal/vertical orientations: Notch becomes Dynamic/`logos`, while `logos` and `simple` retain their modes. The pickup command resolves when native tracking starts, not when the drag finishes.

The same screen owner polls the pointer every 16 ms during a drag and reads the primary button through public macOS `NSEvent.pressedMouseButtons` or Windows `GetAsyncKeyState`, respecting Windows swapped-button settings. Every preview and the final release use the same nearest-edge/grip-fraction formula: the visible HUD stays docked along an edge of its own screen, retaining the original edge on an exact tie. There is no free canvas movement through the desktop. The captured grip determines position along the selected edge. The owner never moves onto another screen or replaces another screen's preferences. macOS also sets public `NSCursor.closedHandCursor` during native tracking and restores the arrow at completion/cancellation, without activating the app.

The first native preview always emits `hud:drag-preview {screenId, mode, edge, offset, monitorIndex}`, including a stationary pickup; subsequent previews emit only when that placement changes. Notch becomes `logos` in the first preview. During pickup the canvas temporarily expands to its owner's full screen, keeping the native frame fixed while React previews different edges. Release or cancellation restores the original narrower canvas dimensions. The native frame and transient geometry are updated before `hud:geometry` and the preview notification. HUDSurface filters by its own `screenId` and passes a temporary render configuration to HUDScene, which disables placement tweening while dragging. Preview notifications do not update saved preferences. The original configuration and display snapshot remain available for cancellation rollback.

A successful release saves the final placement as the screen's mode, edge and offset, then emits `hud:drag-end {screenId, cancelled:false}`. Configure or native-error cancellation restores the original configuration/frame before ending; an explicit new configuration then takes effect. Hiding/disconnection clears the temporary drag state, and disconnected owners are destroyed. Cancellation emits `{screenId, cancelled:true}` and releases drag input ownership. Existing visible windows then follow their reported regions again. HUD frontends filter geometry, preview and drag-end events by their URL's `screenId`, and clear temporary rendering at drag end. A DOM mouse-up is not required to finish native tracking. Linux has no native drag backend yet.

## Disconnect and reconnect

The host's planner is the sole display-list owner and shares `hud:monitors` with the active windows when the list changes; pages read `hud_monitors` when they open or come back.

When the connected set changes, the planner ends the owners of disconnected screens before configuring the connected ones: it removes them from the registry, stops their workers, makes their windows pass-through, and hides/destroys them. Pending requests against stopped owners are rejected. This prevents an old HUD from remaining on another display after the OS rearranges windows during disconnection.

The presentation owner retains disconnected screens' saved preferences. When a Mac display reconnects with the same UUID, the planner creates a new owner/window for that same key and restores its preferences using the current monitor index. No such cross-session identity guarantee is made for the current Windows/Linux fallback IDs.

## Platform boundaries and validation

- **macOS:** Public Rust `objc2` AppKit calls set status-bar level + 1, `CanJoinAllSpaces`, `Stationary`, `FullScreenAuxiliary`, `IgnoresCycle`, transparency and no hide-on-deactivate. `orderFrontRegardless` avoids Tao's `makeKeyAndOrderFront` show path. A pinned creation-time Tao patch creates only explicitly marked HUD windows as `TaoNonactivatingPanel : NSPanel`, retaining Tao's ivar, event forwarding and normal close/drop ownership. Ordinary windows keep `TaoWindow : NSWindow`. The initial nonactivating style mask and temporary text-input key lease support underlying-application keyboard handoff without activating Agent HUD. No runtime class replacement or delegate replacement is used. `nativeNonactivatingPanel` and `keyboardHandoff` report this Mac implementation; actual focus/IME handoff still requires native GUI acceptance. See `docs/tao-panel-patch.md` for details. Fullscreen, Spaces, focus, actual notch alignment, mixed DPI and hotplug behavior still require native GUI qualification.
- **Windows:** Uses Tauri borderless/transparent/non-focusable/always-on-top and cursor-passthrough APIs, plus the public primary-button query for dragging. Unique monitor device paths identify saved layouts. All-virtual-desktop visibility is unsupported by Tauri; exclusive fullscreen and secure desktops are not covered. Native keyboard focus handoff and hardware-notch measurement are not implemented. Native builds/tests run on GitHub Windows runners; GUI behavior still requires Windows machine acceptance.
- **Linux X11:** Transparency, stacking, edge placement and workspace behavior depend on the window manager/compositor. IDs are session-only, and native HUD dragging is unsupported. Linux builds and GUI behavior have not been qualified by the Mac compile check.
- **Linux Wayland:** With wlr-layer-shell (`gtk-layer-shell`), each HUD is an overlay layer surface that covers its whole output and never moves or resizes; the front end draws the strip inside it, so a surface-relative pointer never feeds its own motion back into a drag. Ignoring the cursor limits input to the reported regions instead of emptying it, so the surface still hears the pointer return. There is no global cursor: hover and drag use the WebView's own GTK pointer and button events, and the pointer is unknown while it is outside every HUD. A text-focus lease switches the surface's keyboard mode to on-demand. Compositors without layer-shell (GNOME) return explicit unsupported errors for visible edge HUD configuration and region tracking. No genuine global cursor coordinates are fabricated from Tao's `(0, 0)` fallback. `GDK_BACKEND=x11` selects the X11 path. Only niri with one output at scale 1 has been tried by hand.

`hud_native_capabilities` reports these boundaries independently of window creation, including `perScreenWindows: true`, `singleWindow: false`, Mac hardware-notch measurement and Mac/Windows persistent-display identity support. A capability is not evidence of GUI parity. The separate explicit system/hook commands can change login and delegated client settings; simply querying HUD capabilities does not invoke them or start the original app.

`cargo check -p agent-hud-host` checks the host and native AppKit adapter on the current Mac target without starting a GUI. Compilation does not verify the native behaviors listed above or cross-compile the other desktop targets.

## Prepared launch routes

Run from the repository root after frontend QA. These commands launch the new app; they are not part of the compile check.

```sh
# Development: Tauri owns its Vite dev server and native process.
npm run tauri --workspace @agent-hud/desktop -- dev

# Alternative: build a debug .app, then launch that exact Next bundle.
npm run tauri --workspace @agent-hud/desktop -- build --debug --bundles app
open "$PWD/target/debug/bundle/macos/Agent HUD.app"
```

Use the new Next bundle. This workflow does not launch the original Agent HUD or install its client hooks. The app configures each connected screen's HUD itself. Statistics opens independently through its command or tray action. Settings and Statistics use Overlay title bars with hidden titles and app-owned drag regions.


## OS operations and owned WebView snapshots

`system/` implements actual readback of login-item support/state and notification authorization. Explicit Settings actions use `SMAppService` on macOS, a current-user Run value on Windows, or an owned XDG autostart entry on Linux. Sound and notifications use the corresponding OS adapter; a regular snapshot uses the cached projection and adds no OS subprocess per collection pass. No fixture registered a login item or requested notification permission.

`native_capture_webview` accepts only the dedicated `hud-picture` label. macOS uses `WKWebView.takeSnapshot` with `afterScreenUpdates=false`; Windows uses WebView2 `CapturePreview`; Linux uses WebKitGTK's visible WebView snapshot. All return bounded PNG bytes from that WebView, never a desktop screenshot. Mac compilation and a Windows COM API source cross-check passed; actual hidden-window capture remains native GUI QA, and Linux host compilation is unverified.
