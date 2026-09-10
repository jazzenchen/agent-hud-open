import Foundation

public enum TokenFormat {
    /// 48_000 → "48k", 1_250 → "1.3k", 950 → "950", 2_400_000 → "2.4M".
    public static func short(_ tokens: Int) -> String {
        let value = Double(tokens)
        if value >= 1_000_000 { return trim(value / 1_000_000) + "M" }
        if value >= 1_000 { return trim(value / 1_000) + "k" }
        return "\(tokens)"
    }

    /// "48k ↓ 12k ↑"
    public static func inOut(in tokensIn: Int, out tokensOut: Int) -> String {
        "\(short(tokensIn)) ↓ \(short(tokensOut)) ↑"
    }

    /// Percent with no decimals: 72.4 → "72%".
    public static func percent(_ value: Double) -> String {
        "\(Int(value.rounded()))%"
    }

    /// Percent with one decimal: 6.2 → "6.2%".
    public static func percent1(_ value: Double) -> String {
        String(format: "%.1f%%", value)
    }

    private static func trim(_ value: Double) -> String {
        value >= 10 ? String(format: "%.0f", value) : String(format: "%.1f", value)
    }
}
