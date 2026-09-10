import Foundation

/// UI language. `.system` follows the user's preferred languages (Chinese → Simplified Chinese, otherwise English).
public enum AppLanguage: String, Codable, Sendable, CaseIterable {
    case system
    case zhHans = "zh-Hans"
    case en
}

public enum ResolvedLanguage: Sendable {
    case zhHans
    case en
}

/// Runtime string selection. Strings are kept in code as (Chinese, English) pairs so both languages sit side by side
/// and switching needs no restart. Persisted identifiers (agent sources, quota window rows) are stored as keys and
/// turned into text through `sourceLabel` / `modelLabel`.
public enum L10n {
    private final class State: @unchecked Sendable {
        let lock = NSLock()
        var language: AppLanguage = .system
    }

    private static let state = State()

    public static func setLanguage(_ language: AppLanguage) {
        state.lock.withLock { state.language = language }
    }

    public static var language: AppLanguage {
        state.lock.withLock { state.language }
    }

    public static var resolved: ResolvedLanguage {
        switch language {
        case .zhHans: return .zhHans
        case .en: return .en
        case .system: return systemLanguage()
        }
    }

    public static func systemLanguage(preferred: [String] = Locale.preferredLanguages) -> ResolvedLanguage {
        guard let first = preferred.first?.lowercased() else { return .en }
        return first.hasPrefix("zh") ? .zhHans : .en
    }

    /// Picks the string for the current language.
    public static func text(_ zh: String, _ en: String) -> String {
        resolved == .zhHans ? zh : en
    }

    // MARK: Persisted keys → display text

    public static func vendorLabel(_ vendor: String) -> String {
        vendor
    }

    public static let sourceClaudeCode = "claude-code"
    public static let sourceClaudeSessions = "claude-sessions"
    public static let sourceBrowserAuth = "browser-auth"
    public static let sourceCodexAppServer = "codex-app-server"
    public static let sourceAdditionalUsage = "existing-login-usage"
    public static let sourceDeepSeekSessions = "deepseek-sessions"
    public static let sourceNotConnected = "not-connected"

    public static func sourceLabel(_ key: String) -> String {
        switch key {
        case sourceClaudeCode: return "Claude Code"
        case sourceClaudeSessions: return text("Claude Code 会话", "Claude Code sessions")
        case sourceBrowserAuth: return text("需浏览器授权", "Needs browser auth")
        case sourceCodexAppServer: return text("本地客户端", "Local client")
        case sourceAdditionalUsage: return text("本地客户端", "Local client")
        case sourceDeepSeekSessions: return text("API 余额、费用与本地会话", "API balance, costs and local sessions")
        case sourceNotConnected: return text("未连接", "Not connected")
        default: return key
        }
    }

    public static let windowSession = "window.session"
    public static let windowWeekly = "window.weekly"
    public static let windowWeeklyPrefix = "window.weekly."

    /// Full row label: "当前会话 · 5h" / "Session · 5h", "本周 · Fable" / "Weekly · Fable"; real model names pass through.
    public static func modelLabel(_ model: String) -> String {
        if model == "Desktop / CLI" || model == "CLI" { return text("账户额度", "Account quota") }
        if model == windowSession { return text("当前会话 · 5h", "Session · 5h") }
        if model == windowWeekly { return text("本周 · 全部模型", "Weekly · all models") }
        if model.hasPrefix(windowWeeklyPrefix) {
            let family = model.dropFirst(windowWeeklyPrefix.count)
            return text("本周 · \(family)", "Weekly · \(family)")
        }
        return model
    }

    /// Short form for titles: "当前会话" / "Session", "本周 Fable" / "Weekly Fable", "Opus 4.5" → "Opus".
    public static func shortModelLabel(_ model: String) -> String {
        if model == "Desktop / CLI" || model == "CLI" { return modelLabel(model) }
        if model == windowSession { return text("当前会话", "Session") }
        if model == windowWeekly { return text("本周", "Weekly") }
        if model.hasPrefix(windowWeeklyPrefix) {
            let family = model.dropFirst(windowWeeklyPrefix.count)
            return text("本周 \(family)", "Weekly \(family)")
        }
        return model.split(separator: " ").first.map(String.init) ?? model
    }

    /// Sunday first, matching `Calendar.component(.weekday)`.
    public static var weekdayNames: [String] {
        resolved == .zhHans
            ? ["周日", "周一", "周二", "周三", "周四", "周五", "周六"]
            : ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    }

    /// Monday first, for the heatmap rows.
    public static var weekdayNamesMondayFirst: [String] {
        let names = weekdayNames
        return Array(names[1...]) + [names[0]]
    }
}
