# Agent HUD browser extension

Pick an element or an area of a page and send it to an agent session through Agent HUD's inspector (Settings → Experimental).

- **Toolbar button** (or a shortcut set in `chrome://extensions/shortcuts`): pick mode. The element under the pointer is outlined; a click picks it, a drag takes an area, Esc lets go.
- **What goes:** a picture cropped from the visible tab, the page's title and address, and for an element its selector, HTML, words and the computed styles that make its look. It lands in the draft and the HUD brings the inspector up.
- **How it reaches the app:** Chrome's native messaging. Agent HUD writes the host's note (`app.agenthud.inspect.json`) into each Chromium browser's `NativeMessagingHosts` folder at every launch, naming the app itself as the host; the app, started by Chrome with the extension's origin, passes each message to the running app over `browser/tools` in its data folder (`src/crates/host/src/inspect/browser.rs`).

## Try it

1. Open Agent HUD once, so it writes the host's note.
2. Add it to Chrome from the [Chrome Web Store](https://chromewebstore.google.com/detail/agent-hud/eaihfcfnoaafgcggajlbegjnfjinkckk). To run this folder's copy instead, open `chrome://extensions`, turn on Developer mode, press **Load unpacked** and choose this folder.
3. Press the toolbar button on any page.

The `key` in `manifest.json` fixes the id of the copy loaded from this folder (`iihocfnohlhbepcckimekpoioandkpkl`); the Chrome Web Store's copy has its own (`eaihfcfnoaafgcggajlbegjnfjinkckk`, `STORE_ID` in `src/crates/host/src/inspect/hosts.rs`). The host's note allows both. Windows is not served yet: Chrome reads the note's path from the registry there.
