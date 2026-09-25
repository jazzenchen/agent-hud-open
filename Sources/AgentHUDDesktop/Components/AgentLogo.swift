import AppKit
import SwiftUI
import AgentHUDCore

/// Bundled brand artwork used by SwiftUI views and the native menu.
enum AgentArtwork {
    /// Vendor to bundled artwork. Shared with the logo queue, which samples the same files at its own size.
    static let fileNames: [String: String] =
        ["Claude": "claude", "ChatGPT": "chatgpt", "Antigravity": "antigravity", "DeepSeek": "deepseek", "Grok": "grok",
         "Cursor": "cursor", "OpenCode": "opencode", "OpenCode-dark": "opencode-dark", "Kimi": "kimi", "GLM": "glm", "Pi": "pi",
         "GitHub Copilot": "copilot", "OpenClaw": "openclaw", "Hermes": "hermes", "ZCode": "zcode", "CodeBuddy": "codebuddy",
         "WorkBuddy": "workbuddy", "Qwen": "qwen", "Qoder": "qoder", "Qoder-dark": "qoder-dark"]

    // A mark that is named here but not bundled must not take the app down with it: the vendor falls back to its
    // lettered badge instead, which is what a client whose artwork nobody has drawn yet already gets.
    @MainActor private static let images: [String: NSImage] = Dictionary(uniqueKeysWithValues:
        fileNames.compactMap { vendor, name -> (String, NSImage)? in
            guard let url = AppResources.bundle.url(forResource: name, withExtension: "png"),
                  let artwork = NSImage(contentsOf: url) else { return nil }
            let image = NSImage(size: NSSize(width: 16, height: 16), flipped: false) { rect in
                artwork.draw(in: rect)
                if vendor == "ChatGPT" {
                    NSColor(srgbRed: 16 / 255, green: 163 / 255, blue: 127 / 255, alpha: 1).setFill()
                    rect.fill(using: .sourceAtop)
                }
                return true
            }
            image.isTemplate = ["Grok", "Cursor", "Kimi", "Pi", "GitHub Copilot", "Hermes", "ZCode"].contains(vendor)
            return (vendor, image)
        }
    )

    /// The bundled artwork at its own resolution, for surfaces that draw the mark larger than the 16 pt
    /// menu icon — the logo queue scales it up rather than blowing up the icon-sized copy.
    @MainActor private static var originals: [String: NSImage] = [:]

    /// - light: which background the mark will sit on. OpenCode ships a light and a dark variant; everything
    ///   else is the same artwork either way.
    @MainActor
    static func original(for vendor: String, light: Bool = false) -> NSImage? {
        let vendor = canonical(vendor, light: light)
        if let hit = originals[vendor] { return hit }
        guard let name = fileNames[vendor],
              let url = AppResources.bundle.url(forResource: name, withExtension: "png"),
              let artwork = NSImage(contentsOf: url)
        else { return nil }
        // OpenAI ships its mark in black, which would disappear on a dark desktop; the app's brand colour
        // for it is the same green the menu uses.
        let image = vendor == "ChatGPT" ? tinted(artwork, with: openAIGreen) : artwork
        image.isTemplate = monochromeVendors.contains(vendor)
        originals[vendor] = image
        return image
    }

    /// Marks that ship as a single colour and have to be tinted to the surface they sit on.
    private static let monochromeVendors: Set<String> =
        ["Grok", "Cursor", "Kimi", "Pi", "GitHub Copilot", "Hermes", "ZCode"]

    private static let openAIGreen = NSColor(srgbRed: 16 / 255, green: 163 / 255, blue: 127 / 255, alpha: 1)

    private static func tinted(_ artwork: NSImage, with color: NSColor) -> NSImage {
        NSImage(size: artwork.size, flipped: false) { rect in
            artwork.draw(in: rect)
            color.setFill()
            rect.fill(using: .sourceAtop)
            return true
        }
    }

    /// True when the artwork is a single-colour mark that must be tinted to be visible. Several vendors ship
    /// black on transparent, which would vanish against a dark desktop.
    static func isTemplate(_ vendor: String) -> Bool {
        monochromeVendors.contains(canonical(vendor))
    }

    /// Vendors whose mark is bundled, for surfaces that show the whole catalog. The second copy of a mark that
    /// ships in two variants is not a vendor of its own.
    static var allVendors: [String] {
        fileNames.keys.filter { !$0.hasSuffix("-dark") }.sorted()
    }

    /// The mark a vendor is drawn with, as a key: vendors that share one piece of artwork share this one.
    /// The logo queue collapses on it rather than on the vendor name, because two identical marks in a row
    /// are two the eye cannot tell apart, and telling agents apart at a glance is what the queue is for.
    static func markKey(_ vendor: String) -> String { canonical(vendor) }

    /// Codex and ChatGPT share the bundled OpenAI artwork, while keeping distinct names, and so do the three Qoder
    /// builds. OpenCode and Qoder ship one file per background, because part of each mark is the background's own
    /// colour and would disappear on the other one.
    private static func canonical(_ vendor: String, light: Bool = false) -> String {
        switch vendor {
        case "Codex": return "ChatGPT"
        case "OpenCode", "OpenCode Go": return light ? "OpenCode" : "OpenCode-dark"
        case "Qoder", "Qoder CN", "QoderWork": return light ? "Qoder" : "Qoder-dark"
        default: return vendor
        }
    }

    @MainActor
    static func image(for vendor: String, dark: Bool? = nil) -> NSImage? {
        let isDark = dark ?? (NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua)
        return images[canonical(vendor, light: !isDark)]
    }

    @MainActor private static var accents: [String: NSColor?] = [:]

    /// The colour a vendor's mark is mostly drawn in: the average of its commonest saturated hue, so a many-coloured
    /// mark gives one of its colours rather than a blend of them. nil for monochrome marks and vendors without one.
    @MainActor
    static func accent(for vendor: String) -> NSColor? {
        if let hit = accents[vendor] { return hit }
        var accent: NSColor?
        if let image = original(for: vendor), !image.isTemplate,
           let context = CGContext(data: nil, width: 32, height: 32, bitsPerComponent: 8, bytesPerRow: 128,
                                   space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) {
            NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
            image.draw(in: CGRect(x: 0, y: 0, width: 32, height: 32))
            NSGraphicsContext.current = nil
            var hues = [(count: Int, red: Double, green: Double, blue: Double)](repeating: (0, 0, 0, 0), count: 12)
            if let pixels = context.data?.assumingMemoryBound(to: UInt8.self) {
                for offset in stride(from: 0, to: 32 * 32 * 4, by: 4) where pixels[offset + 3] > 128 {
                    let alpha = Double(pixels[offset + 3])
                    let red = Double(pixels[offset]) / alpha, green = Double(pixels[offset + 1]) / alpha, blue = Double(pixels[offset + 2]) / alpha
                    let color = NSColor(srgbRed: red, green: green, blue: blue, alpha: 1)
                    guard color.saturationComponent > 0.35, color.brightnessComponent > 0.3 else { continue }
                    let bin = min(11, Int(color.hueComponent * 12))
                    hues[bin].count += 1; hues[bin].red += red; hues[bin].green += green; hues[bin].blue += blue
                }
            }
            if let top = hues.max(by: { $0.count < $1.count }), top.count > 0 {
                let count = Double(top.count)
                accent = NSColor(srgbRed: top.red / count, green: top.green / count, blue: top.blue / count, alpha: 1)
            }
        }
        accents[vendor] = accent
        return accent
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
        } else if let initial = vendor.first {
            // No bundled mark: a lettered one, the way the phone already does it. Drawing nothing would leave the
            // row looking like it belongs to whoever is above it.
            Text(String(initial).uppercased())
                .font(.system(size: size * 0.6, weight: .semibold))
                .frame(width: size, height: size)
                .background(Color.secondary.opacity(0.25), in: RoundedRectangle(cornerRadius: size * 0.3))
                .accessibilityHidden(true)
        }
    }
}
