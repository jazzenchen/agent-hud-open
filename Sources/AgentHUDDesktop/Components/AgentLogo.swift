import AppKit
import SwiftUI
import AgentHUDCore

/// Bundled brand artwork used by SwiftUI views and the native menu.
enum AgentArtwork {
    private static let images: [String: NSImage] = Dictionary(uniqueKeysWithValues:
        ["Claude": "claude", "ChatGPT": "chatgpt", "Antigravity": "antigravity", "DeepSeek": "deepseek", "Grok": "grok",
         "Cursor": "cursor", "OpenCode": "opencode", "OpenCode-dark": "opencode-dark", "Kimi": "kimi", "GLM": "glm", "Pi": "pi"].map { vendor, name in
            let artwork = NSImage(contentsOf: AppResources.bundle.url(forResource: name, withExtension: "png")!)!
            let image = NSImage(size: NSSize(width: 16, height: 16), flipped: false) { rect in
                artwork.draw(in: rect)
                if vendor == "ChatGPT" {
                    NSColor(srgbRed: 16 / 255, green: 163 / 255, blue: 127 / 255, alpha: 1).setFill()
                    rect.fill(using: .sourceAtop)
                }
                return true
            }
            image.isTemplate = ["Grok", "Cursor", "Kimi", "Pi"].contains(vendor)
            return (vendor, image)
        }
    )

    @MainActor
    static func image(for vendor: String, dark: Bool? = nil) -> NSImage? {
        // Codex and ChatGPT share the bundled OpenAI artwork, while keeping distinct names.
        if vendor == "OpenCode" || vendor == "OpenCode Go" {
            let isDark = dark ?? (NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua)
            return images[isDark ? "OpenCode-dark" : "OpenCode"]
        }
        return images[vendor == "Codex" ? "ChatGPT" : vendor]
    }
}

struct AgentLogo: View {
    let vendor: String
    var size: CGFloat = 16
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        if let image = AgentArtwork.image(for: vendor, dark: colorScheme == .dark) {
            Image(nsImage: image)
                .renderingMode(image.isTemplate ? .template : .original)
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
                .accessibilityHidden(true)
        }
    }
}
