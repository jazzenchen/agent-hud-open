import Foundation

public enum PollInterval: Int, Codable, Sendable, CaseIterable {
        case thirtySeconds = 30
        case oneMinute = 60
        case fiveMinutes = 300
}

public enum AppearanceMode: String, Codable, Sendable, CaseIterable {
        case system
        case dark
        case light
}

/// User-tunable settings. Every field has a default so older stored JSON still decodes.
public struct Settings: Hashable, Codable, Sendable {
    public static let glowSizeRange: ClosedRange<Double> = 0...20

    public var breathSeconds: Double = 3
    /// 0…1. Glow opacity oscillates between `1 - amplitude` and 1 (times brightness).
    public var breathAmplitude: Double = 0.6
    public var glowRange: Double = 14
    public var glowBlur: Double = 8
    /// Keep the island's rim dense and fade outward with distance.
    public var glowOutwardOnly: Bool = true
    /// 0.2…1
    public var glowBrightness: Double = 0.9
    public var hoverDelayMs: Int = 400
    public var collapseDelayMs: Int = 200
    public var showResetCountdown: Bool = true
    public var showIslandQuota: Bool = true
    public var showIslandTokens: Bool = true
    public var showIslandSessions: Bool = true
    /// Agent vendors whose live status is excluded from presentation, relay and completion reminders.
    /// Collection, session history and token accounting are independent of this preference.
    public private(set) var disabledLiveStatusSources: Set<String> = []
    public var pollInterval: PollInterval = .oneMinute
    public var launchAtLogin: Bool = true
    public var showMenuBarIcon: Bool = true
    public var appearance: AppearanceMode = .system
    public var language: AppLanguage = .system

    public init() {}

    private enum CodingKeys: String, CodingKey {
        case breathSeconds, breathAmplitude, glowRange, glowBlur, glowBrightness, glowOutwardOnly
        case hoverDelayMs, collapseDelayMs, showResetCountdown, pollInterval
        case showIslandQuota, showIslandTokens, showIslandSessions
        case disabledLiveStatusSources
        case launchAtLogin, showMenuBarIcon, appearance, language
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Settings()
        breathSeconds = try c.decodeIfPresent(Double.self, forKey: .breathSeconds) ?? d.breathSeconds
        breathAmplitude = try c.decodeIfPresent(Double.self, forKey: .breathAmplitude) ?? d.breathAmplitude
        glowRange = Self.clampGlowSize(try c.decodeIfPresent(Double.self, forKey: .glowRange) ?? d.glowRange)
        glowBlur = Self.clampGlowSize(try c.decodeIfPresent(Double.self, forKey: .glowBlur) ?? d.glowBlur)
        glowOutwardOnly = try c.decodeIfPresent(Bool.self, forKey: .glowOutwardOnly) ?? d.glowOutwardOnly
        glowBrightness = try c.decodeIfPresent(Double.self, forKey: .glowBrightness) ?? d.glowBrightness
        hoverDelayMs = try c.decodeIfPresent(Int.self, forKey: .hoverDelayMs) ?? d.hoverDelayMs
        collapseDelayMs = try c.decodeIfPresent(Int.self, forKey: .collapseDelayMs) ?? d.collapseDelayMs
        showResetCountdown = try c.decodeIfPresent(Bool.self, forKey: .showResetCountdown) ?? d.showResetCountdown
        showIslandQuota = try c.decodeIfPresent(Bool.self, forKey: .showIslandQuota) ?? d.showIslandQuota
        showIslandTokens = try c.decodeIfPresent(Bool.self, forKey: .showIslandTokens) ?? d.showIslandTokens
        showIslandSessions = try c.decodeIfPresent(Bool.self, forKey: .showIslandSessions) ?? d.showIslandSessions
        disabledLiveStatusSources = Set((try c.decodeIfPresent([String].self, forKey: .disabledLiveStatusSources) ?? []).map { $0.lowercased() })
        pollInterval = try c.decodeIfPresent(PollInterval.self, forKey: .pollInterval) ?? d.pollInterval
        launchAtLogin = try c.decodeIfPresent(Bool.self, forKey: .launchAtLogin) ?? d.launchAtLogin
        showMenuBarIcon = try c.decodeIfPresent(Bool.self, forKey: .showMenuBarIcon) ?? d.showMenuBarIcon
        appearance = try c.decodeIfPresent(AppearanceMode.self, forKey: .appearance) ?? d.appearance
        language = try c.decodeIfPresent(AppLanguage.self, forKey: .language) ?? d.language
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(breathSeconds, forKey: .breathSeconds)
        try c.encode(breathAmplitude, forKey: .breathAmplitude)
        try c.encode(glowRange, forKey: .glowRange)
        try c.encode(glowBlur, forKey: .glowBlur)
        try c.encode(glowBrightness, forKey: .glowBrightness)
        try c.encode(glowOutwardOnly, forKey: .glowOutwardOnly)
        try c.encode(hoverDelayMs, forKey: .hoverDelayMs)
        try c.encode(collapseDelayMs, forKey: .collapseDelayMs)
        try c.encode(showResetCountdown, forKey: .showResetCountdown)
        try c.encode(pollInterval, forKey: .pollInterval)
        try c.encode(showIslandQuota, forKey: .showIslandQuota)
        try c.encode(showIslandTokens, forKey: .showIslandTokens)
        try c.encode(showIslandSessions, forKey: .showIslandSessions)
        try c.encode(disabledLiveStatusSources.sorted(), forKey: .disabledLiveStatusSources)
        try c.encode(launchAtLogin, forKey: .launchAtLogin)
        try c.encode(showMenuBarIcon, forKey: .showMenuBarIcon)
        try c.encode(appearance, forKey: .appearance)
        try c.encode(language, forKey: .language)
    }

    private static func clampGlowSize(_ value: Double) -> Double {
        min(glowSizeRange.upperBound, max(glowSizeRange.lowerBound, value))
    }

    /// Functional update helper so call sites read as `settings.with { $0.glowRange = 20 }`.
    public func with(_ change: (inout Settings) -> Void) -> Settings {
        var copy = self
        change(&copy)
        return copy
    }

    public var hoverDelay: TimeInterval { Double(hoverDelayMs) / 1000 }
    public var collapseDelay: TimeInterval { Double(collapseDelayMs) / 1000 }

    public func liveStatusEnabled(for vendor: String) -> Bool {
        !disabledLiveStatusSources.contains(vendor.lowercased())
    }

    public mutating func setLiveStatus(for vendor: String, enabled: Bool) {
        if enabled { disabledLiveStatusSources.remove(vendor.lowercased()) }
        else { disabledLiveStatusSources.insert(vendor.lowercased()) }
    }
}
