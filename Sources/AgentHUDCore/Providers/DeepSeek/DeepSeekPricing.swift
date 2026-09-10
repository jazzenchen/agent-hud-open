import Foundation

/// Official prices verified 2026-09-07. Estimates are local Harness usage, not account invoices.
/// https://api-docs.deepseek.com/zh-cn/quick_start/pricing/
/// https://api-docs.deepseek.com/quick_start/pricing/
public enum DeepSeekPricing {
    public static let checkedOn = "2026-09-07"
    public static let sourceURL = URL(string: "https://api-docs.deepseek.com/quick_start/pricing/")!

    public static func isPeak(_ date: Date) -> Bool {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 8 * 3600)!
        let day = calendar.component(.weekday, from: date), hour = calendar.component(.hour, from: date)
        return (2...6).contains(day) && ((9..<12).contains(hour) || (14..<18).contains(hour))
    }

    public static func estimate(_ usage: DeepSeekTranscript.Usage, currency: String) -> Decimal? {
        guard usage.provider == "deepseek-official" else { return nil }
        let prices: (String, String, String)
        switch (usage.model, currency) {
        case ("deepseek-v4-flash", "CNY"), ("deepseek-v4-flash-vision-exp", "CNY"): prices = ("1.5", "0.05", "4.5")
        case ("deepseek-v4-pro", "CNY"): prices = ("4.5", "0.15", "13.5")
        case ("deepseek-v4-flash", "USD"), ("deepseek-v4-flash-vision-exp", "USD"): prices = ("0.22", "0.007", "0.66")
        case ("deepseek-v4-pro", "USD"): prices = ("0.66", "0.022", "1.98")
        default: return nil
        }
        let amount = Decimal(usage.input) * Decimal(string: prices.0)!
            + Decimal(usage.cachedInput) * Decimal(string: prices.1)!
            + Decimal(usage.output) * Decimal(string: prices.2)!
        return amount * (isPeak(usage.requestedAt) ? 2 : 1) / 1_000_000
    }
}
