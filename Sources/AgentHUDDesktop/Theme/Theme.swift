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

public extension Font {
    /// System font (SF Pro) at the design's sizes.
    static func ui(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    static func tabular(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight).monospacedDigit()
    }
}

/// Design tokens for the dark and light window chrome. The island/glow always use `.island` (dark).
public struct Theme {
    public let isLight: Bool
    public let windowBackground: Color
    public let sidebarBackground: Color
    public let sidebarBorder: Color
    public let inputBackground: Color
    public let inputBorder: Color
    public let text: Color
    public let secondary: Color
    public let tertiary: Color
    public let card: Color
    public let cardBorder: Color
    public let divider: Color
    public let track: Color
    public let rowBackground: Color
    public let rowBorder: Color
    public let sessionRowBackground: Color
    public let segmentBackground: Color
    public let segmentSelected: Color
    public let sidebarSelected: Color
    public let sidebarText: Color
    public let sidebarSelectedText: Color
    public let dotEnded: Color
    public let toggleOff: Color
    public let chipOff: Color
    public let windowShadowOpacity: Double

    public let accent = Color(hex: 0x0a84ff)
    public let toggleOn = Color(hex: 0x30d158)
    public let warningText = Color(hex: 0xc7a100)

    public func status(_ level: StatusLevel) -> Color {
        Color(StatusPalette.color(for: level, light: isLight))
    }

    public func statusText(_ level: StatusLevel) -> Color {
        Color(StatusPalette.textColor(for: level, light: isLight))
    }

    public static let dark = Theme(
        isLight: false,
        windowBackground: Color(hex: 0x282828),
        sidebarBackground: Color(hex: 0x1f1f1f),
        sidebarBorder: Color.white.opacity(0.08),
        inputBackground: Color(hex: 0x1c1c1e),
        inputBorder: Color.white.opacity(0.14),
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
        sidebarSelected: Color.white.opacity(0.12),
        sidebarText: Color(hex: 0xd0d0d5),
        sidebarSelectedText: Color.white,
        dotEnded: Color(hex: 0x6e6e73),
        toggleOff: Color.white.opacity(0.2),
        chipOff: Color.white.opacity(0.12),
        windowShadowOpacity: 0.5
    )

    public static let light = Theme(
        isLight: true,
        windowBackground: Color(hex: 0xf5f5f7),
        sidebarBackground: Color(hex: 0xe8e8ea),
        sidebarBorder: Color.black.opacity(0.08),
        inputBackground: Color.white,
        inputBorder: Color.black.opacity(0.15),
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
        sidebarSelected: Color.black.opacity(0.08),
        sidebarText: Color(hex: 0x1d1d1f),
        sidebarSelectedText: Color(hex: 0x1d1d1f),
        dotEnded: Color(hex: 0xaeaeb2),
        toggleOff: Color.black.opacity(0.2),
        chipOff: Color.black.opacity(0.08),
        windowShadowOpacity: 0.35
    )

    /// The island panel is always black with dark-mode tokens.
    public static let island = Theme.dark

    public static func forScheme(_ scheme: ColorScheme) -> Theme {
        scheme == .light ? .light : .dark
    }
}

/// Convenience: `Color(rgba)` for agent palette entries.
extension AgentPalette {
    static func swiftUIColor(index: Int) -> Color {
        Color(color(index: index))
    }
}
