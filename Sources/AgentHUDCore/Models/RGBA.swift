import Foundation

/// Device-independent color. The core never imports AppKit/SwiftUI, so colors travel as plain numbers.
public struct RGBA: Hashable, Codable, Sendable {
    public let red: Double
    public let green: Double
    public let blue: Double
    public let alpha: Double

    public init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    /// `RGBA(hex: 0x3ddc84)`
    public init(hex: UInt32, alpha: Double = 1) {
        self.init(
            red: Double((hex >> 16) & 0xff) / 255,
            green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255,
            alpha: alpha
        )
    }

    public func withAlpha(_ alpha: Double) -> RGBA {
        RGBA(red: red, green: green, blue: blue, alpha: alpha)
    }

    /// CSS-style hex, e.g. `#3ddc84`. Alpha is dropped.
    public var hexString: String {
        let r = Int((red * 255).rounded()), g = Int((green * 255).rounded()), b = Int((blue * 255).rounded())
        return String(format: "#%02x%02x%02x", r, g, b)
    }

    public static let white = RGBA(hex: 0xffffff)
    public static let black = RGBA(hex: 0x000000)
}
