# Original Agent HUD artwork

These files are unchanged copies from
`agent-hud-native-open/Sources/AgentHUDDesktop/Resources/`.

The 22 provider PNGs are the exact files named by the original desktop
`Components/AgentLogo.swift` artwork map, including the OpenCode and Qoder dark
variants. Codex and ChatGPT share `chatgpt.png`. In the original renderer,
ChatGPT's mark is tinted `#10a37f`; Grok, Cursor, Kimi, Pi, GitHub Copilot, Hermes
and ZCode are template images tinted to their foreground. The source bitmaps
are preserved here without applying those rendering transformations.

`minimax.png` and `ark.png` (Volcengine's mark, for Volcengine Ark) were added
later, unchanged from LobeHub's `@lobehub/icons-static-png` 2.6.0
(`light/minimax-color.png`, `light/volcengine-color.png`; MIT, the notice in
`LobeIcons-LICENSE.txt`).

The bundled `clawd-sprites.json` is consumed by the original Clawd bot renderer.
It contains Anthropic's artwork copied unchanged from Claude for macOS
2.37337.1's Clawd Quest sprite table. Copyright Anthropic PBC, all rights
reserved; this artwork is **not covered by this repository's license**. See
the Clawd entry in the copied `THIRD_PARTY_NOTICES.txt` for its original source
and extraction details. The other original bots are drawn procedurally by
`Bots/BotRenderer.swift` and related source files and have no raster sprite
assets to copy.

Provider marks retain their respective owners' branding. The original
LobeHub icon notice and full MIT text are preserved in
`LobeIcons-LICENSE.txt`. `THIRD_PARTY_NOTICES.txt` is also retained verbatim as
the upstream attribution record; its code-parser notices describe the
original application and do not imply that all of those parsers are used by
this UI package.

This asset inventory establishes the original visual sources. A React
renderer must still reproduce original layout, tinting and animation behavior
before visual parity can be claimed.
