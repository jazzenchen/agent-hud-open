# Brand assets

## Overview

Which client logos the application bundles, where each one came from, how it is rendered, and under which terms. Logos are loaded from the application's resource bundle; no network request is made at runtime.

## Lobe Icons

Claude, OpenAI (shown as ChatGPT), Antigravity, DeepSeek, Grok, GitHub Copilot, OpenClaw, Hermes Agent, CodeBuddy, Qwen Code and Qoder come from [Lobe Icons](https://github.com/lobehub/lobe-icons), package `@lobehub/icons-static-png` version `1.97.0`.

| Bundled file | Original file in the package |
| --- | --- |
| `claude.png` | `light/claude-color.png` |
| `chatgpt.png` | `light/openai.png` |
| `antigravity.png` | `light/antigravity-color.png` |
| `deepseek.png` | `light/deepseek-color.png` |
| `grok.png` | `light/grok.png` |
| `copilot.png` | `light/githubcopilot.png` |
| `openclaw.png` | `light/openclaw-color.png` |
| `hermes.png` | `light/hermesagent.png` |
| `codebuddy.png` | `light/codebuddy-color.png` |
| `qwen.png` | `light/qwen-color.png` |
| `qoder.png`, `qoder-dark.png` | `light/qoder-color.png`, `dark/qoder-color.png` |

The PNG files are unchanged (640 × 640); only the names differ: the `-color` suffix is dropped, and `openai.png`, `githubcopilot.png` and `hermesagent.png` are stored as `chatgpt.png`, `copilot.png` and `hermes.png`. Codex rows reuse the ChatGPT artwork under their own name. Claude, Antigravity, DeepSeek, OpenClaw, CodeBuddy and Qwen Code keep their source colors; the OpenAI mark is stored as shipped and tinted green (`#10A37F`) at render time for ChatGPT; the monochrome Grok, GitHub Copilot and Hermes Agent marks are rendered as template images, so they follow the foreground color in light and dark appearance. Qoder ships one file per background, because the light half of its mark is the background's own colour; the three Qoder builds share both files. The package's MIT license is bundled as `LobeIcons-LICENSE.txt`.

## Cursor, OpenCode, Kimi, GLM, Pi, ZCode and WorkBuddy

These seven use the clients' official artwork instead of SF Symbols.

| Client | Source | Bundled file | Rendering |
| --- | --- | --- | --- |
| Cursor | [Official brand assets](https://cursor.com/brand), `General Logos/Cube/SVG/CUBE_2D_DARK.svg` in the downloadable archive | `cursor.png` | Monochrome template |
| OpenCode / OpenCode Go | [Official brand page](https://opencode.ai/brand); [square logos at commit 830d5eb](https://github.com/anomalyco/opencode/tree/830d5eb5354874105cc31599635a80c1662609e8/packages/console/app/src/asset/brand) | `opencode.png`, `opencode-dark.png` | Original artwork; the light or dark file is chosen by the current appearance |
| Kimi | [Official branding guide](https://moonshotai.github.io/Branding-Guide/), `scenarios/04-k-only/k-only-color.svg` | `kimi.png` | Monochrome template |
| GLM | [Z.ai](https://chat.z.ai/) linked [brand icon](https://z-cdn.chatglm.cn/z-ai/static/logo.svg) | `glm.png` | Original artwork |
| Pi | [Official press kit](https://pi.dev/press-kit), [primary logo](https://pi.dev/logo-auto.svg) | `pi.png` | Monochrome template |
| ZCode | [Official site](https://zcode.z.ai/en), the Z mark inside the header logo (inline SVG) | `zcode.png` | Monochrome template |
| WorkBuddy | [Official site](https://www.workbuddy.ai/), its [site icon](https://download.codebuddy.ai/web/workbuddy/35f50f59737cd16a3a0f458d5719ce972b630a2f/assets/logo.svg) | `workbuddy.png` | Original artwork |

The bundled files are 256 × 256 transparent PNG renders of those SVGs, fitted and centered without changing their proportions. Pi's excess transparent canvas is trimmed before fitting, without changing the mark's geometry. The original SVGs are not distributed in this repository.

## Rendering rules

Every logo is drawn into a 16 × 16 point image. Grok, Cursor, Kimi, Pi, GitHub Copilot, Hermes Agent and ZCode are template images (monochrome, foreground-colored); Claude, ChatGPT (tinted), Antigravity, DeepSeek, GLM, OpenCode, OpenClaw, CodeBuddy, WorkBuddy, Qwen Code and Qoder are drawn as original images. SwiftUI views and the status-item menu use the same images.

## Licenses and trademarks

Lobe Icons are MIT-licensed (`LobeIcons-LICENSE.txt`). Brand names and marks belong to their respective owners and are used only to identify the corresponding client. Provider protocol references are listed separately in `THIRD_PARTY_NOTICES.txt`.

## Code map

| Concept | Code |
| --- | --- |
| Bundled logos and license files | `Sources/AgentHUDDesktop/Resources/` |
| Resource lookup and logo drawing | `Sources/AgentHUDDesktop/Components/AppResources.swift`, `AgentLogo.swift` |

## Related

[architecture.md](architecture.md#design-invariants) resources and notices · [../THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt)
