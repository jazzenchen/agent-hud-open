# UI source provenance

- React, React DOM, Vite and the Tauri API/CLI are resolved from the npm `latest` stable tags and pinned by the lockfile.
- shadcn/ui: current components generated with CLI `4.22.1` from https://ui.shadcn.com. MIT. Shared components live in `src/packages/ui/src/ui`.
- MCP: official `@modelcontextprotocol/server` / `client` `2.3.1`, current stable v2 line for the `2026-07-28` specification. https://ts.sdk.modelcontextprotocol.io/v2/.

Registry-generated utility imports are adapted to `@agent-hud/ui/lib/utils` so the UI package does not import app-private modules. Shader animation is disabled with reduced motion or when the document is hidden.

- Original Agent HUD UI layouts, geometry, hover behavior and bot logic are ports from `agent-hud-native-open`, licensed under Apache-2.0. The license is included at the repository root in [LICENSE](../LICENSE). Native rendering expressions were translated to React, Canvas, SVG and Rust.
- Original provider PNGs and the Clawd sprites are retained under src/packages/hud/src/assets with original notices; the Settings wallpaper is in src/packages/ui/src/assets and the application icon in src/packages/app/src/assets. Clawd sprites retain their separate Anthropic copyright; see src/packages/hud/src/assets/README.md and THIRD_PARTY_NOTICES.txt.

shadcn visual overrides live in src/packages/ui/src/global.css; registry component behavior is unchanged. HUD geometry and timelines are product implementation code. The HUD rim's WGSL shader implements the original six glow styles and six effects and is drawn with WebGPU directly (`src/packages/hud/src/glow-gpu.ts`); it keeps a static fallback when WebGPU is unavailable.
