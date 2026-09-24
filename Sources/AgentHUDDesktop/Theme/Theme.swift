import AppKit
import SwiftUI
import AgentHUDCore

extension Color {
    init(_ rgba: RGBA) {
        self.init(.sRGB, red: rgba.red, green: rgba.green, blue: rgba.blue, opacity: rgba.alpha)
    }

    init(hex: UInt32, opacity: Double = 1) {
        self.init(RGBA(hex: hex, alpha: opacity))
    }
}

extension NSColor {
    convenience init(_ rgba: RGBA) {
        self.init(srgbRed: rgba.red, green: rgba.green, blue: rgba.blue, alpha: rgba.alpha)
    }

    convenience init(hex: UInt32, alpha: Double = 1) {
        self.init(RGBA(hex: hex, alpha: alpha))
    }
}

extension CGColor {
    static func rgba(_ c: RGBA) -> CGColor {
        CGColor(colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!, components: [c.red, c.green, c.blue, c.alpha])!
    }
}

extension Font {
    /// System font (SF Pro) at the design's sizes.
    static func ui(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    static func tabular(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight).monospacedDigit()
    }
}

/// Design tokens for the dark and light window chrome. The island/glow always use `.island` (dark).
struct Theme {
    /// Chooses the status palette variant.
    private let isLight: Bool
    let windowBackground: Color
    let sidebarBackground: Color
    let sidebarBorder: Color
    let inputBackground: Color
    let text: Color
    let secondary: Color
    let tertiary: Color
    let card: Color
    let cardBorder: Color
    let divider: Color
    let track: Color
    let rowBackground: Color
    let rowBorder: Color
    let sessionRowBackground: Color
    let segmentBackground: Color
    let segmentSelected: Color
    let sidebarText: Color
    let dotEnded: Color

    func status(_ level: StatusLevel) -> Color {
        Color(StatusPalette.color(for: level, light: isLight))
    }

    func statusText(_ level: StatusLevel) -> Color {
        Color(StatusPalette.textColor(for: level, light: isLight))
    }

    /// One hue per token kind, lighter on the dark chrome.
    func kind(_ kind: TokenKind) -> Color {
        switch kind {
        case .cacheWrite: Color(hex: isLight ? 0x9470cd : 0xb699eb)
        case .input: Color(hex: isLight ? 0x398ad6 : 0x6db0f4)
        case .reasoning: Color(hex: isLight ? 0xc35f92 : 0xe38ab5)
        case .output: Color(hex: isLight ? 0xc26e12 : 0xe29858)
        case .cacheRead: Color(hex: isLight ? 0x00a091 : 0x31c3b5)
        }
    }

    static let dark = Theme(
        isLight: false,
        windowBackground: Color(hex: 0x282828),
        sidebarBackground: Color(hex: 0x1f1f1f),
        sidebarBorder: Color.white.opacity(0.08),
        inputBackground: Color(hex: 0x1c1c1e),
        text: Color(hex: 0xf5f5f7),
        secondary: Color(hex: 0x98989d),
        tertiary: Color(hex: 0x6e6e73),
        card: Color.white.opacity(0.05),
        cardBorder: Color.white.opacity(0.12),
        divider: Color.white.opacity(0.10),
        track: Color.white.opacity(0.12),
        rowBackground: Color.white.opacity(0.05),
        rowBorder: Color.clear,
        sessionRowBackground: Color.white.opacity(0.04),
        segmentBackground: Color.white.opacity(0.08),
        segmentSelected: Color(hex: 0x5a5a5e),
        sidebarText: Color(hex: 0xd0d0d5),
        dotEnded: Color(hex: 0x6e6e73)
    )

    static let light = Theme(
        isLight: true,
        windowBackground: Color(hex: 0xf5f5f7),
        sidebarBackground: Color(hex: 0xe8e8ea),
        sidebarBorder: Color.black.opacity(0.08),
        inputBackground: Color.white,
        text: Color(hex: 0x1d1d1f),
        secondary: Color(hex: 0x6e6e73),
        tertiary: Color(hex: 0x8e8e93),
        card: Color.white,
        cardBorder: Color.black.opacity(0.06),
        divider: Color.black.opacity(0.08),
        track: Color.black.opacity(0.10),
        rowBackground: Color.white,
        rowBorder: Color.black.opacity(0.06),
        sessionRowBackground: Color.black.opacity(0.03),
        segmentBackground: Color.black.opacity(0.08),
        segmentSelected: Color.white,
        sidebarText: Color(hex: 0x1d1d1f),
        dotEnded: Color(hex: 0xaeaeb2)
    )

    /// The island panel is always black with dark-mode tokens.
    static let island = Theme.dark

    static func forScheme(_ scheme: ColorScheme) -> Theme {
        scheme == .light ? .light : .dark
    }
}

/// Convenience: `Color(rgba)` for agent palette entries.
extension AgentPalette {
    static func swiftUIColor(index: Int) -> Color {
        Color(color(index: index))
    }
}
