import Foundation

public enum AppearanceMode: String, Codable, Sendable, CaseIterable {
        case system
        case dark
        case light
}

/// How the notch glow is drawn.
public enum GlowStyle: String, Codable, Sendable, CaseIterable {
    /// The blurred band from the design.
    case blur
    /// A halftone grid: dots shrink as the glow fades.
    case dots
    /// The same grid drawn with ASCII characters ordered by density.
    case ascii
    /// Shade blocks ░▒▓ with a solid cell where the glow is strongest.
    case blocks
    /// Braille characters whose eight dots switch on by ordered dithering.
    case braille
    /// Ones and zeros dithered from the glow; zeros stay dim.
    case binary
}

/// What the dot and ASCII glow styles do while an agent is running.
public enum GlowEffect: String, Codable, Sendable, CaseIterable {
    /// Dots grow and shrink with the breath period and depth.
    case breathe
    /// The colour gradient drifts back and forth along the notch.
    case flow
    /// A bright band sweeps from left to right.
    case scan
    /// Brightness waves travel outward from the island.
    case ripple
    /// Cells twinkle with per-cell noise.
    case shimmer
    /// The glow grows out from the island's edge, holds, then fades.
    case boot
}

/// User-tunable settings. Every field has a default so older stored JSON still decodes.
public struct Settings: Hashable, Codable, Sendable {
    public static let glowSizeRange = GlowSettings.sizeRange
    public static let breathSecondsRange = GlowSettings.breathSecondsRange
    public static let glowGridPitchRange = GlowSettings.gridPitchRange
    public static let glowGridCoreRange = GlowSettings.gridCoreRange
    public static let glowGridFadeRange = GlowSettings.gridFadeRange
    public static let glowGridDensityRange = GlowSettings.gridDensityRange

    /// The glow every screen falls back to. A display with its own is in `screenGlow`.
    public var glow = GlowSettings()
    /// Per-display glow, keyed the same way as `screens`. A HUD belongs to a screen, so what it is made of
    /// belongs to that screen too: the notch's rim and an external display's backdrop want different things.
    public var screenGlow: [String: GlowSettings] = [:]

    /// The glow for one display, or the default when it has none of its own.
    public func glow(on screen: String?) -> GlowSettings {
        screen.flatMap { screenGlow[$0] } ?? glow
    }

    // Flat accessors onto the default glow, so anything that means "the glow" rather than "this screen's
    // glow" — previews, onboarding, stored JSON — keeps reading and writing one place.
    public var breathSeconds: Double {
        get { glow.breathSeconds } set { glow.breathSeconds = newValue }
    }
    public var idleBreathSeconds: Double {
        get { glow.idleBreathSeconds } set { glow.idleBreathSeconds = newValue }
    }
    public var breathAmplitude: Double {
        get { glow.breathAmplitude } set { glow.breathAmplitude = newValue }
    }
    public var glowRange: Double { get { glow.range } set { glow.range = newValue } }
    public var glowBlur: Double { get { glow.blur } set { glow.blur = newValue } }
    public var glowOutwardOnly: Bool { get { glow.outwardOnly } set { glow.outwardOnly = newValue } }
    public var glowBrightness: Double { get { glow.brightness } set { glow.brightness = newValue } }
    public var glowStyle: GlowStyle { get { glow.style } set { glow.style = newValue } }
    public var glowGridPitch: Double { get { glow.gridPitch } set { glow.gridPitch = newValue } }
    public var glowGridCore: Double { get { glow.gridCore } set { glow.gridCore = newValue } }
    public var glowGridFade: Double { get { glow.gridFade } set { glow.gridFade = newValue } }
    public var glowGridDensity: Double { get { glow.gridDensity } set { glow.gridDensity = newValue } }
    public var glowEffect: GlowEffect { get { glow.effect } set { glow.effect = newValue } }
    /// Hovering alone opens the panel. With this on it takes Option as well, so a HUD parked over the menu
    /// bar or a window's title bar does not open every time the pointer crosses it.
    public var requiresOptionToOpen: Bool = false
    public var hoverDelayMs: Int = 400
    public var collapseDelayMs: Int = 200
    public var showResetCountdown: Bool = true
    public var showIslandQuota: Bool = true
    public var showIslandTokens: Bool = true
    public var showIslandSessions: Bool = true
    /// Agent vendors whose live status is excluded from presentation, relay and completion reminders.
    /// Collection, session history and token accounting are independent of this preference.
    public private(set) var disabledLiveStatusSources: Set<String> = []
    /// Querying GitHub Copilot quota reads the GitHub CLI sign-in, so it stays off until the user agrees.
    public var readCopilotQuota: Bool = false
    /// Whether Agent HUD keeps its handlers in the clients' own settings: approvals, Claude Code's notification
    /// hook, the stop hooks and Pi's observer. Off, they are removed and never added back.
    public var clientHooks: Bool = true
    /// How long a permission request waits on the HUD for an answer before it goes back to the client's own prompt.
    public var approvalWaitMinutes: Int = 10
    public static let approvalWaitChoices = [1, 3, 5, 10, 30, 60]
    public var launchAtLogin: Bool = true
    public var showMenuBarIcon: Bool = true
    public var appearance: AppearanceMode = .system
    public var language: AppLanguage = .system
    /// Per-screen HUD placement, keyed by the display's stable UUID. A screen missing from the map takes
    /// `ScreenPlacement.default(hasNotch:)` for its hardware, so a newly attached display needs no setup.
    public var screens: [String: ScreenPlacement] = [:]

    /// This screen's placement, or the default its hardware deserves. The companion to `glow(on:)`: both
    /// resolve "what this display uses" in one place, so a caller cannot invent its own fallback.
    public func placement(on screen: String, hasNotch: Bool) -> ScreenPlacement {
        screens[screen] ?? .default(hasNotch: hasNotch)
    }

    public init() {}

    private enum CodingKeys: String, CodingKey {
        case breathSeconds, idleBreathSeconds, breathAmplitude, glowRange, glowBlur, glowBrightness, glowOutwardOnly
        case glowStyle, glowGridPitch, glowGridSpread, glowGridCore, glowGridFade, glowGridDensity, glowEffect
        case requiresOptionToOpen, hoverDelayMs, collapseDelayMs, showResetCountdown
        case showIslandQuota, showIslandTokens, showIslandSessions
        case disabledLiveStatusSources, readCopilotQuota, clientHooks, approvalWaitMinutes
        case launchAtLogin, showMenuBarIcon, appearance, language, screens, screenGlow
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Settings()
        breathSeconds = try c.decodeIfPresent(Double.self, forKey: .breathSeconds) ?? d.breathSeconds
        idleBreathSeconds = Self.clamp(try c.decodeIfPresent(Double.self, forKey: .idleBreathSeconds), to: Self.breathSecondsRange, default: d.idleBreathSeconds)
        breathAmplitude = try c.decodeIfPresent(Double.self, forKey: .breathAmplitude) ?? d.breathAmplitude
        glowRange = Self.clampGlowSize(try c.decodeIfPresent(Double.self, forKey: .glowRange) ?? d.glowRange)
        glowBlur = Self.clampGlowSize(try c.decodeIfPresent(Double.self, forKey: .glowBlur) ?? d.glowBlur)
        glowOutwardOnly = try c.decodeIfPresent(Bool.self, forKey: .glowOutwardOnly) ?? d.glowOutwardOnly
        glowBrightness = try c.decodeIfPresent(Double.self, forKey: .glowBrightness) ?? d.glowBrightness
        // A style saved by a newer build falls back to the blurred glow instead of failing the whole decode.
        glowStyle = (try? c.decodeIfPresent(GlowStyle.self, forKey: .glowStyle)) ?? d.glowStyle
        glowGridPitch = Self.clamp(try c.decodeIfPresent(Double.self, forKey: .glowGridPitch), to: Self.glowGridPitchRange, default: d.glowGridPitch)
        glowGridCore = Self.clamp(try c.decodeIfPresent(Double.self, forKey: .glowGridCore), to: Self.glowGridCoreRange, default: d.glowGridCore)
        // Settings written before the falloff was split carry one decay length. Twice it lands on a fade
        // that reads like the exponential curve it replaced.
        let storedFade = try c.decodeIfPresent(Double.self, forKey: .glowGridFade)
            ?? (try c.decodeIfPresent(Double.self, forKey: .glowGridSpread)).map { $0 * 2 }
        glowGridFade = Self.clamp(storedFade, to: Self.glowGridFadeRange, default: d.glowGridFade)
        glowGridDensity = Self.clamp(try c.decodeIfPresent(Double.self, forKey: .glowGridDensity), to: Self.glowGridDensityRange, default: d.glowGridDensity)
        glowEffect = (try? c.decodeIfPresent(GlowEffect.self, forKey: .glowEffect)) ?? d.glowEffect
        requiresOptionToOpen = try c.decodeIfPresent(Bool.self, forKey: .requiresOptionToOpen) ?? d.requiresOptionToOpen
        hoverDelayMs = try c.decodeIfPresent(Int.self, forKey: .hoverDelayMs) ?? d.hoverDelayMs
        collapseDelayMs = try c.decodeIfPresent(Int.self, forKey: .collapseDelayMs) ?? d.collapseDelayMs
        showResetCountdown = try c.decodeIfPresent(Bool.self, forKey: .showResetCountdown) ?? d.showResetCountdown
        showIslandQuota = try c.decodeIfPresent(Bool.self, forKey: .showIslandQuota) ?? d.showIslandQuota
        showIslandTokens = try c.decodeIfPresent(Bool.self, forKey: .showIslandTokens) ?? d.showIslandTokens
        showIslandSessions = try c.decodeIfPresent(Bool.self, forKey: .showIslandSessions) ?? d.showIslandSessions
        disabledLiveStatusSources = Set((try c.decodeIfPresent([String].self, forKey: .disabledLiveStatusSources) ?? []).map { $0.lowercased() })
        readCopilotQuota = try c.decodeIfPresent(Bool.self, forKey: .readCopilotQuota) ?? d.readCopilotQuota
        clientHooks = try c.decodeIfPresent(Bool.self, forKey: .clientHooks) ?? d.clientHooks
        approvalWaitMinutes = (try c.decodeIfPresent(Int.self, forKey: .approvalWaitMinutes))
            .flatMap { Self.approvalWaitChoices.contains($0) ? $0 : nil } ?? d.approvalWaitMinutes
        launchAtLogin = try c.decodeIfPresent(Bool.self, forKey: .launchAtLogin) ?? d.launchAtLogin
        showMenuBarIcon = try c.decodeIfPresent(Bool.self, forKey: .showMenuBarIcon) ?? d.showMenuBarIcon
        appearance = try c.decodeIfPresent(AppearanceMode.self, forKey: .appearance) ?? d.appearance
        language = try c.decodeIfPresent(AppLanguage.self, forKey: .language) ?? d.language
        screens = (try? c.decodeIfPresent([String: ScreenPlacement].self, forKey: .screens)) ?? d.screens
        screenGlow = (try? c.decodeIfPresent([String: GlowSettings].self, forKey: .screenGlow)) ?? d.screenGlow
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(breathSeconds, forKey: .breathSeconds)
        try c.encode(idleBreathSeconds, forKey: .idleBreathSeconds)
        try c.encode(breathAmplitude, forKey: .breathAmplitude)
        try c.encode(glowRange, forKey: .glowRange)
        try c.encode(glowBlur, forKey: .glowBlur)
        try c.encode(glowBrightness, forKey: .glowBrightness)
        try c.encode(glowOutwardOnly, forKey: .glowOutwardOnly)
        try c.encode(glowStyle, forKey: .glowStyle)
        try c.encode(glowGridPitch, forKey: .glowGridPitch)
        try c.encode(glowGridCore, forKey: .glowGridCore)
        try c.encode(glowGridFade, forKey: .glowGridFade)
        try c.encode(glowGridDensity, forKey: .glowGridDensity)
        try c.encode(glowEffect, forKey: .glowEffect)
        try c.encode(requiresOptionToOpen, forKey: .requiresOptionToOpen)
        try c.encode(hoverDelayMs, forKey: .hoverDelayMs)
        try c.encode(collapseDelayMs, forKey: .collapseDelayMs)
        try c.encode(showResetCountdown, forKey: .showResetCountdown)
        try c.encode(showIslandQuota, forKey: .showIslandQuota)
        try c.encode(showIslandTokens, forKey: .showIslandTokens)
        try c.encode(showIslandSessions, forKey: .showIslandSessions)
        try c.encode(disabledLiveStatusSources.sorted(), forKey: .disabledLiveStatusSources)
        try c.encode(readCopilotQuota, forKey: .readCopilotQuota)
        try c.encode(clientHooks, forKey: .clientHooks)
        try c.encode(approvalWaitMinutes, forKey: .approvalWaitMinutes)
        try c.encode(launchAtLogin, forKey: .launchAtLogin)
        try c.encode(showMenuBarIcon, forKey: .showMenuBarIcon)
        try c.encode(appearance, forKey: .appearance)
        try c.encode(language, forKey: .language)
        try c.encode(screens, forKey: .screens)
        try c.encode(screenGlow, forKey: .screenGlow)
    }

    private static func clampGlowSize(_ value: Double) -> Double {
        min(glowSizeRange.upperBound, max(glowSizeRange.lowerBound, value))
    }

    private static func clamp(_ value: Double?, to range: ClosedRange<Double>, default fallback: Double) -> Double {
        guard let value, value.isFinite else { return fallback }
        return min(range.upperBound, max(range.lowerBound, value))
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
