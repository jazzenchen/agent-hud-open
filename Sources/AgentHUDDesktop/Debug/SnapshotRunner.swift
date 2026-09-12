import AppKit
import SwiftUI
import AgentHUDCore

/// `--snapshot <dir>`: renders every screen at 2× to PNG for visual verification against the design.
/// Views are hosted in a real (off-screen) window so AppKit-backed controls such as sliders render too.
@MainActor
public enum SnapshotRunner {
    public static func run(language: AppLanguage? = nil, into directory: String) async {
        // Independent demo stores so snapshots always show the design's data, whatever the user configured.
        let defaults = UserDefaults(suiteName: "app.agenthud.open.snapshot")!
        defaults.removePersistentDomain(forName: "app.agenthud.open.snapshot")
        let settings = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        if let language = language {
            settings.update { $0.language = language }
            L10n.setLanguage(language)
        }
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date()))

        let folder = URL(fileURLWithPath: directory, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)

        let alertAgent = AgentDescriptor(id: "claude-session", vendor: "Claude", model: L10n.windowSession, source: "", enabled: true)
        for kind in QuotaAlert.Kind.allCases {
            let alert = IslandAlert.quota(QuotaAlert.preview(kind, agent: alertAgent))
            save("alert-\(kind.rawValue)-compact", IslandScene(store: store, settings: settings, open: false, light: false, alert: alert), folder: folder, scheme: .dark)
            save("alert-\(kind.rawValue)-detail", IslandScene(store: store, settings: settings, open: true, light: false, alert: alert, showsAlertDetails: true), folder: folder, scheme: .dark)
            save("alert-\(kind.rawValue)-inline", IslandScene(store: store, settings: settings, open: true, light: false, alert: alert), folder: folder, scheme: .dark)
        }

        for vendor in ["Claude", "Codex", "DeepSeek"] {
            let now = Date()
            let completion = SessionCompletion(sessionID: "snapshot", vendor: vendor, turnID: "preview",
                task: L10n.text("完成本地用量面板", "Build the local usage dashboard"),
                model: ["Claude": "Fable 5.1", "Codex": "gpt-6-astra"][vendor] ?? "deepseek-v4-flash", startedAt: now.addingTimeInterval(-83), completedAt: now)
            let alert = IslandAlert.completion(completion, preview: true)
            save("alert-completion-\(vendor)-compact", IslandScene(store: store, settings: settings, open: false, light: false, alert: alert), folder: folder, scheme: .dark)
            save("alert-completion-\(vendor)-detail", IslandScene(store: store, settings: settings, open: true, light: false, alert: alert, showsAlertDetails: true), folder: folder, scheme: .dark)
            save("alert-completion-\(vendor)-inline", IslandScene(store: store, settings: settings, open: true, light: false, alert: alert), folder: folder, scheme: .dark)
        }

        save("island-collapsed", IslandScene(store: store, settings: settings, open: false, light: false), folder: folder, scheme: .dark)
        save("island-expanded-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        save("island-expanded-light", IslandScene(store: store, settings: settings, open: true, light: true), folder: folder, scheme: .light)
        save("menubar-dark", MenuBarStrip(store: store, light: false), folder: folder, scheme: .dark)
        save("menubar-light", MenuBarStrip(store: store, light: true), folder: folder, scheme: .light)
        for tab in SettingsTab.allCases {
            save("settings-\(tab.slug)-dark", SettingsView(settings: settings, store: store, initialTab: tab).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark)
            save("settings-\(tab.slug)-light", SettingsView(settings: settings, store: store, initialTab: tab).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .light)
            save("settings-\(tab.slug)-small-dark", SettingsView(settings: settings, store: store, initialTab: tab).frame(width: SettingsWindowLayout.minimum.width, height: SettingsWindowLayout.minimum.height), folder: folder, scheme: .dark)
        }
        await saveAgentSettings(settings: settings, store: store, folder: folder)
        save("settings-display-bottom-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark, scrollToBottom: true)
        save("settings-display-small-bottom-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.minimum.width, height: SettingsWindowLayout.minimum.height), folder: folder, scheme: .dark, scrollToBottom: true)
        let sampleWidth = SettingsWindowLayout.size.width - SettingsWindowLayout.sidebarWidth - 1
        let smallSampleWidth = SettingsWindowLayout.minimum.width - SettingsWindowLayout.sidebarWidth - 1
        save("settings-panel-samples-dark", IslandPane(settings: settings, theme: .dark).padding(20).frame(width: sampleWidth).background(Theme.dark.windowBackground), folder: folder, scheme: .dark)
        save("settings-panel-samples-small-dark", IslandPane(settings: settings, theme: .dark).padding(20).frame(width: smallSampleWidth).background(Theme.dark.windowBackground), folder: folder, scheme: .dark)
        save("settings-panel-samples-light", IslandPane(settings: settings, theme: .light).padding(20).frame(width: sampleWidth).background(Theme.light.windowBackground), folder: folder, scheme: .light)
        settings.update {
            $0.showIslandQuota = false; $0.showIslandTokens = false; $0.showIslandSessions = false
            $0.showResetCountdown = false
        }
        save("settings-panel-samples-off-dark", IslandPane(settings: settings, theme: .dark).padding(20).frame(width: sampleWidth).background(Theme.dark.windowBackground), folder: folder, scheme: .dark)
        settings.update {
            $0.showIslandQuota = true; $0.showIslandTokens = true; $0.showIslandSessions = true
            $0.showResetCountdown = true
        }
        settings.update { $0.glowOutwardOnly = false }
        save("settings-display-soft-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark)
        settings.update { $0.glowOutwardOnly = Settings().glowOutwardOnly }
        save("onboarding-dark", OnboardingView(settings: settings, store: store, sources: DemoData.sources, onFinish: {}), folder: folder, scheme: .dark)
        save("onboarding-light", OnboardingView(settings: settings, store: store, sources: DemoData.sources, onFinish: {}), folder: folder, scheme: .light)
        save("stats-dark", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .dark)
        save("stats-light", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .light)
        let dashboardAgents = settings.agents
        settings.updateAgents { $0.filter { ["Claude", "Codex", "DeepSeek"].contains($0.vendor) } }
        let dashboardStore = UsageStore(provider: DemoUsageProvider(), settings: settings)
        let dashboardReport = DemoUsageProvider.report(agents: settings.agents, historyHours: UsageStore.historyHours, now: Date())
        dashboardStore.replace(report: UsageReport(generatedAt: dashboardReport.generatedAt,
            snapshots: dashboardReport.snapshots, sessions: dashboardReport.sessions, history: dashboardReport.history,
            activity: dashboardReport.activity, insights: dashboardReport.insights, consumers: dashboardReport.consumers,
            consumption: dashboardReport.consumption,
            insightsByAgent: Dictionary(uniqueKeysWithValues: dashboardReport.snapshots.map { ($0.agentId, dashboardReport.insights) }),
            subscriptions: dashboardReport.subscriptions,
            billing: [DemoData.deepSeekBilling(now: dashboardReport.generatedAt)]))
        save("stats-dashboard-dark", StatsView(store: dashboardStore).frame(width: 960, height: 900), folder: folder, scheme: .dark)
        save("stats-dashboard-light", StatsView(store: dashboardStore).frame(width: 760, height: 700), folder: folder, scheme: .light)
        save("stats-dashboard-bottom-dark", StatsView(store: dashboardStore).frame(width: 960, height: 640), folder: folder, scheme: .dark, scrollToBottom: true)
        save("stats-dashboard-full-dark", StatsView(store: dashboardStore, scrollable: false).frame(width: 960), folder: folder, scheme: .dark)
        dashboardStore.setStatsRange(.days7)
        dashboardStore.tokenBucketSize = .day1
        save("stats-dashboard-daily-dark", StatsView(store: dashboardStore).frame(width: 960, height: 900), folder: folder, scheme: .dark)
        await saveAdaptiveDashboard(store: dashboardStore, folder: folder)
        settings.updateAgents { _ in dashboardAgents }
        store.tokenDimensions = .cache
        save("stats-cache-dark", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .dark)
        store.tokenDimensions = .all
        save("stats-all-tokens-light", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .light)
        store.tokenDimensions = .fresh
        let heatmapModels: [(String, String, Int)] = [
            ("Claude", "Fable 5.1", 2_480_112), ("Claude", "Opus 5", 1_150_320),
            ("Claude", "Opus 4.8", 824_521), ("Claude", "Fable 5", 390_064),
            ("Codex", "codex-auto-review", 72_150), ("Codex", "gpt-5.5", 10_890),
            ("Codex", "gpt-5.6-sol", 940), ("Codex", "gpt-6-astra", 310),
            ("DeepSeek", "deepseek-v4-flash", 1),
        ]
        let heatmapConsumers = heatmapModels.map { AgentDescriptor(id: $0.1, vendor: $0.0, model: $0.1, source: "Snapshot", enabled: true) }
        let heatmapCounts = Dictionary(uniqueKeysWithValues: heatmapModels.map { ($0.1, $0.2) })
        for scheme in [ColorScheme.dark, .light] {
            let theme = Theme.forScheme(scheme)
            save("heatmap-models-\(scheme == .dark ? "dark" : "light")",
                 HeatmapModelDetails(period: "Mon 21:00–22:00", tokensByModel: heatmapCounts, consumers: heatmapConsumers, theme: theme)
                    .frame(width: 300).padding(12).background(theme.card), folder: folder, scheme: scheme)
        }
        save("stats-wide-dark", StatsView(store: store, scrollable: false).frame(width: 1200), folder: folder, scheme: .dark)
        save("stats-wide-light", StatsView(store: store, scrollable: false).frame(width: 1200), folder: folder, scheme: .light)
        save("stats-wide-bottom-dark", StatsView(store: store).frame(width: 1200, height: 640), folder: folder, scheme: .dark, scrollToBottom: true)
        for range in StatsRange.allCases {
            store.setStatsRange(range)
            save("stats-\(range.hours)h-dark", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .dark)
            for bucket in TokenBucketSize.allCases {
                store.tokenBucketSize = bucket
                save("stats-\(range.hours)h-\(bucket.rawValue)m-dark", StatsView(store: store, scrollable: false).frame(width: 760), folder: folder, scheme: .dark)
            }
        }
        store.setStatsRange(.hours24)
        store.tokenBucketSize = .hour1
        settings.update {
            $0.showIslandTokens = false
        }
        settings.updateAgents { $0.map { $0.with(enabled: $0.id == "claude-opus") } }
        save("island-custom-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showIslandQuota = false; $0.showIslandSessions = false; $0.showIslandTokens = true }
        save("island-tokens-only-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showIslandTokens = false }
        save("island-minimal-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showIslandQuota = true; $0.showIslandTokens = true; $0.showIslandSessions = true }
        settings.updateAgents { $0.map { $0.with(enabled: false) } }
        save("island-no-models-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)

        settings.updateAgents { _ in DemoData.agents + [
            AgentDescriptor(id: "codex-spark-preview", vendor: "Codex", model: "GPT-5.3-Codex-Spark · Weekly · all models", source: L10n.sourceCodexAppServer, enabled: false),
        ] }
        save("settings-sources-bottom-dark", SettingsView(settings: settings, store: store, initialTab: .sources).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark, scrollToBottom: true)
        settings.update { $0.glowRange = 20; $0.glowBlur = 20 }
        save("settings-display-max-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark)
        settings.update { $0.glowRange = 0 }
        save("settings-display-zero-range-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark)
        settings.update { $0.glowBlur = 0 }
        save("settings-display-min-dark", SettingsView(settings: settings, store: store, initialTab: .display).frame(width: SettingsWindowLayout.size.width, height: SettingsWindowLayout.size.height), folder: folder, scheme: .dark)

        // Cover the longer quota labels and weekday reset times shown by real accounts.
        let quotaAgents = [
            AgentDescriptor(id: "claude-session", vendor: "Claude", model: L10n.windowSession, source: L10n.sourceClaudeSessions, enabled: true),
            AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: L10n.windowWeekly, source: L10n.sourceClaudeSessions, enabled: true),
            AgentDescriptor(id: "claude-weekly-fable", vendor: "Claude", model: L10n.windowWeeklyPrefix + "Fable", source: L10n.sourceClaudeSessions, enabled: true),
            AgentDescriptor(id: "codex", vendor: "Codex", model: L10n.windowWeekly, source: L10n.sourceCodexAppServer, enabled: true),
        ]
        settings.updateAgents { _ in quotaAgents }
        settings.update {
            $0.showIslandTokens = false; $0.showIslandSessions = false
            $0.glowRange = Settings().glowRange; $0.glowBlur = Settings().glowBlur
        }
        let resetNow = Date()
        let loadingStore = UsageStore(provider: DemoUsageProvider(), settings: settings)
        save("island-loading-quota", IslandScene(store: loadingStore, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showResetCountdown = false }
        save("island-loading-no-reset", IslandScene(store: loadingStore, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showResetCountdown = true; $0.showIslandTokens = true }
        save("island-loading-tokens", IslandScene(store: loadingStore, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        loadingStore.replace(report: UsageReport(generatedAt: resetNow, snapshots: [], sessions: [], history: [],
                                                activity: .empty, insights: .empty, indexing: IndexProgress(done: 12, total: 80)))
        save("island-loading-indexing", IslandScene(store: loadingStore, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.update { $0.showIslandTokens = false }
        let quotaSnapshots = quotaAgents.enumerated().map { index, agent in
            UsageSnapshot(agentId: agent.id, remainingPct: [88, 21, 40, 74][index],
                          resetAt: resetNow.addingTimeInterval(index == 0 ? 2 * 3600 + 13 * 60 : Double(index + 1) * 86400),
                          windowDuration: index == 0 ? 5 * 3600 : 7 * 86400,
                          updatedAt: resetNow)
        }
        store.replace(report: UsageReport(generatedAt: resetNow, snapshots: quotaSnapshots, sessions: [], history: [], activity: .empty, insights: .empty))
        save("island-weekly-resets-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        let resetBalances: [(String, CodexResetCredits?)] = [
            ("available", DemoData.codexResetCredits(now: resetNow)),
            ("count-only", CodexResetCredits(availableCount: 3, credits: nil)),
            ("partial", CodexResetCredits(availableCount: 5, credits: [
                .init(id: "unknown-expiry", expiresAt: nil),
                .init(id: "known-expiry", expiresAt: resetNow.addingTimeInterval(86400).timeIntervalSince1970),
            ])),
            ("zero", CodexResetCredits(availableCount: 0, credits: [])),
            ("unavailable", nil),
        ]
        for (name, balance) in resetBalances {
            store.replace(report: UsageReport(generatedAt: resetNow, snapshots: quotaSnapshots, sessions: [], history: [],
                                             activity: .empty, insights: .empty, codexResetCredits: balance))
            save("island-reset-\(name)-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
            if let balance, balance.availableCount > 0 {
                save("island-reset-hover-\(name)-dark", ResetCreditsDetails(resets: balance).padding(8).background(Color.black),
                     folder: folder, scheme: .dark)
            }
            if name == "available" {
                save("island-reset-available-light", IslandScene(store: store, settings: settings, open: true, light: true), folder: folder, scheme: .light)
                settings.update { $0.showResetCountdown = false }
                save("island-reset-dates-hidden-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
                settings.update { $0.showResetCountdown = true; $0.showIslandQuota = false }
                save("island-reset-quota-hidden-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
                settings.update { $0.showIslandQuota = true }
                settings.setAgent(id: "codex", enabled: false)
                save("island-reset-codex-hidden-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
                settings.setAgent(id: "codex", enabled: true)
            }
        }
        settings.update { $0.showResetCountdown = false }
        save("island-no-reset-column-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)

        // An API-billed agent has one account card, controlled by its agent switch.
        let deepseek = AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceDeepSeekSessions, enabled: true)
        settings.updateAgents { _ in quotaAgents + [deepseek] }
        settings.update { $0.showResetCountdown = true }
        store.replace(report: UsageReport(generatedAt: resetNow, snapshots: quotaSnapshots, sessions: [], history: [],
                                         activity: .empty, insights: .empty, billing: [DemoData.deepSeekBilling(now: resetNow)],
                                         codexResetCredits: DemoData.codexResetCredits(now: resetNow)))
        save("island-layout-all-agents", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.updateAgents { _ in [quotaAgents[0], deepseek] }
        settings.update { $0.showResetCountdown = true }
        store.replace(report: UsageReport(generatedAt: resetNow, snapshots: [quotaSnapshots[0]], sessions: [], history: [],
                                         activity: .empty, insights: .empty, billing: [DemoData.deepSeekBilling(now: resetNow)]))
        save("island-deepseek-enabled-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.setAgent(id: "deepseek", enabled: false)
        save("island-deepseek-disabled-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)
        settings.updateAgents { _ in [deepseek] }
        save("island-deepseek-only-dark", IslandScene(store: store, settings: settings, open: true, light: false), folder: folder, scheme: .dark)

        await IslandAnimationChecks.run(store: store, settings: settings, folder: folder)
        IslandHoverChecks.run(store: store, settings: settings, folder: folder)
    }

    private static func saveAgentSettings(settings: SettingsStore, store: UsageStore, folder: URL) async {
        if let prefix = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"], !"settings-agents".hasPrefix(prefix) { return }
        let original = settings.agents
        let preferences = settings.settings
        let originalReport = store.report
        defer {
            settings.updateAgents { _ in original }
            settings.update { $0 = preferences }
            if let originalReport { store.replace(report: originalReport) }
        }
        settings.updateAgents { _ in [
            AgentDescriptor(id: "settings-claude-5h", vendor: "Claude", model: L10n.windowSession, source: L10n.sourceClaudeCode, enabled: true),
            AgentDescriptor(id: "settings-claude-week", vendor: "Claude", model: L10n.windowWeekly, source: L10n.sourceClaudeCode, enabled: true),
            AgentDescriptor(id: "settings-claude-model", vendor: "Claude", model: L10n.windowWeeklyPrefix + "Fable", source: L10n.sourceClaudeCode, enabled: false),
            AgentDescriptor(id: "settings-codex", vendor: "Codex", model: "5h", source: L10n.sourceCodexAppServer, enabled: true),
            AgentDescriptor(id: "settings-deepseek-chat", vendor: "DeepSeek", model: "deepseek-chat", source: L10n.sourceDeepSeekSessions, enabled: true),
            AgentDescriptor(id: "settings-deepseek-reasoner", vendor: "DeepSeek", model: "deepseek-reasoner", source: L10n.sourceDeepSeekSessions, enabled: true),
        ] }
        let sources: [SourceStatus] = [
            .init(id: "claude-code", name: "Claude", detail: L10n.text("额度、会话与用量统计", "Quota, sessions and usage"), state: .ready(plan: "max_20x")),
            .init(id: "codex-cli", name: "Codex", detail: L10n.text("额度、会话与用量统计", "Quota, sessions and usage"), state: .ready(plan: "prolite")),
            .init(id: "deepseek", name: "DeepSeek", detail: L10n.text("Harness 会话、API 余额与费用", "Harness sessions, API balance and costs"), state: .ready(plan: nil)),
            .init(id: "antigravity", name: "Antigravity", detail: L10n.text("启动并登录 Antigravity 或 agy 后读取额度 · 部分本地会话无法读取，用量可能不完整", "Start and sign in to Antigravity or agy to load quota. Some local sessions could not be read; usage may be incomplete."), state: .unavailable),
            .init(id: "cursor", name: "Cursor", detail: L10n.text("账户额度与跨设备用量", "Account quota and usage across devices"), state: .notDetected),
            .init(id: "grok", name: "Grok", detail: L10n.text("Grok CLI 额度与本地会话", "Grok CLI quota and local sessions"), state: .ready(plan: "X Premium")),
            .init(id: "opencode", name: "OpenCode", detail: "", state: .installed),
            .init(id: "pi", name: "Pi", detail: "", state: .installed),
            .init(id: "kimi", name: "Kimi", detail: "", state: .ready(plan: "Allegretto")),
            .init(id: "glm", name: "GLM", detail: "", state: .notDetected),
        ]
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [], history: [], activity: .empty,
            insights: .empty, subscriptions: ["kimi-plan": "Allegretto"], billing: [DemoData.deepSeekBilling(now: Date())], services: [
                .init(client: "OpenCode", provider: "Anthropic", product: .api),
                .init(client: "OpenCode", provider: "OpenAI", product: .api),
                .init(client: "OpenCode", provider: "Kimi", product: .plan, accountID: "kimi-plan"),
                .init(client: "Pi", provider: "Anthropic", product: .api),
            ]))
        for scheme in [ColorScheme.dark, .light] {
            let appearance = scheme == .dark ? "dark" : "light"
            for (state, expanded) in [("collapsed", Set<String>()), ("expanded", ["Claude"]), ("prepaid", ["DeepSeek"]), ("empty", ["Antigravity"])] {
                let displayOrder = settings.agents
                if state == "prepaid" { settings.moveAgentGroup(id: "DeepSeek", to: "Claude") }
                defer { settings.updateAgents { _ in displayOrder } }
                let view = SettingsView(settings: settings, store: store, initialTab: .sources,
                                        sourceStatuses: sources, initiallyExpandedAgents: expanded)
                save("settings-agents-\(state)-\(appearance)", view.frame(width: 760, height: 800), folder: folder, scheme: scheme)
                if state == "collapsed" {
                    save("settings-agents-accounts-\(appearance)", view.frame(width: 760, height: 800),
                         folder: folder, scheme: scheme, scrollOffset: 420)
                }
                if state == "expanded" || state == "prepaid" {
                    save("settings-agents-\(state)-small-\(appearance)", view.frame(width: 680, height: 720), folder: folder, scheme: scheme)
                    save("settings-agents-\(state)-models-small-\(appearance)", view.frame(width: 680, height: 720),
                         folder: folder, scheme: scheme, scrollOffset: state == "prepaid" ? 400 : 240)
                }
            }
        }
        if L10n.resolved == .zhHans { await AgentSettingsChecks.run(settings: settings, store: store, sources: sources) }
    }

    private static func saveAdaptiveDashboard(store: UsageStore, folder: URL) async {
        let name = "stats-dashboard-adaptive"
        if let prefix = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"], !name.hasPrefix(prefix) { return }
        let controller = StatsWindowController(store: store)
        guard let window = controller.window, let hosting = window.contentView else { return }
        window.alphaValue = 0
        window.appearance = NSAppearance(named: .darkAqua)
        window.orderBack(nil)
        defer { window.orderOut(nil) }

        // Exercise reflow in both directions in a real window, including the screen-height cap.
        for (index, width) in [StatsWindowLayout.preferredSize.width, 960.0, 1120.0, StatsWindowLayout.preferredSize.width].enumerated() {
            var frame = window.frame
            frame.size.width = width
            window.setFrame(frame, display: true)
            for _ in 0..<6 {
                try? await Task.sleep(for: .milliseconds(50))
                hosting.layoutSubtreeIfNeeded()
            }
            guard let scroll = firstScrollView(in: hosting), let document = scroll.documentView,
                  let screen = window.screen else {
                print("snapshot FAILED \(name): missing scroll view or screen")
                continue
            }
            let overflow = document.bounds.height - scroll.contentView.bounds.height
            let available = screen.visibleFrame.insetBy(dx: 0, dy: 12)
            let capped = abs(window.frame.height - available.height) < 2
            let contained = window.frame.minY >= available.minY - 1 && window.frame.maxY <= available.maxY + 1
            guard contained, capped || abs(overflow) < 2 else {
                print("snapshot FAILED \(name): width=\(width) height=\(window.frame.height) overflow=\(overflow) capped=\(capped)")
                continue
            }
            print("snapshot adaptive PASS: width=\(width) height=\(window.frame.height) overflow=\(overflow) capped=\(capped)")
            capture("\(name)-\(Int(width))-\(index)", view: hosting, folder: folder)
        }

        let originalAgents = store.settings.agents
        store.settings.updateAgents { agents in
            agents + (0..<40).map {
                AgentDescriptor(id: "sizing-\($0)", vendor: "Preview \($0)", model: "Quota", source: "Snapshot", enabled: true)
            }
        }
        for _ in 0..<6 {
            try? await Task.sleep(for: .milliseconds(50))
            hosting.layoutSubtreeIfNeeded()
        }
        if let scroll = firstScrollView(in: hosting), let document = scroll.documentView, let screen = window.screen {
            let capped = abs(window.frame.height - (screen.visibleFrame.height - 24)) < 2
            let overflow = document.bounds.height - scroll.contentView.bounds.height
            print("snapshot adaptive \(capped && overflow > 0 ? "PASS" : "FAILED"): screen cap height=\(window.frame.height) overflow=\(overflow)")
        }
        store.settings.updateAgents { _ in originalAgents }
        for _ in 0..<6 {
            try? await Task.sleep(for: .milliseconds(50))
            hosting.layoutSubtreeIfNeeded()
        }
        if let scroll = firstScrollView(in: hosting), let document = scroll.documentView {
            let overflow = document.bounds.height - scroll.contentView.bounds.height
            print("snapshot adaptive \(abs(overflow) < 2 ? "PASS" : "FAILED"): content restored height=\(window.frame.height) overflow=\(overflow)")
        }
    }

    private static func save<V: View>(_ name: String, _ view: V, folder: URL, scheme: ColorScheme,
                                     scrollToBottom: Bool = false, scrollOffset: CGFloat? = nil) {
        // Limit visual rechecks to the changed surface, e.g. AGENTHUD_SNAPSHOT_PREFIX=settings-.
        if let prefix = ProcessInfo.processInfo.environment["AGENTHUD_SNAPSHOT_PREFIX"], !name.hasPrefix(prefix) { return }
        // Default sizing options so the hosting view reports SwiftUI's ideal size; then freeze it.
        let hosting = NSHostingView(rootView: view.ignoresSafeArea())
        var size = hosting.fittingSize
        if size.width <= 0 || size.height <= 0 { size = hosting.intrinsicContentSize }
        guard size.width > 0, size.height > 0 else {
            print("snapshot FAILED \(name): zero size")
            return
        }
        hosting.sizingOptions = []
        let window = OffscreenWindow(contentRect: CGRect(x: -20000, y: -20000, width: size.width, height: size.height))
        window.appearance = NSAppearance(named: scheme == .dark ? .darkAqua : .aqua)
        hosting.frame = CGRect(origin: .zero, size: size)
        window.contentView = hosting
        window.orderFrontRegardless()
        hosting.layoutSubtreeIfNeeded()
        // Let SwiftUI commit its first transaction.
        RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        if scrollToBottom || scrollOffset != nil {
            guard let scroll = firstScrollView(in: hosting), let document = scroll.documentView,
                  document.bounds.height > scroll.contentView.bounds.height else {
                print("snapshot FAILED \(name): no overflowing scroll view")
                window.orderOut(nil)
                return
            }
            let previous = scroll.contentView.bounds.origin.y
            let bottom = document.isFlipped ? document.bounds.maxY - scroll.contentView.bounds.height : document.bounds.minY
            let destination = scrollOffset.map { min(max(0, $0), document.bounds.height - scroll.contentView.bounds.height) } ?? bottom
            scroll.contentView.scroll(to: CGPoint(x: 0, y: destination))
            scroll.reflectScrolledClipView(scroll.contentView)
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
            guard scroll.contentView.bounds.origin.y != previous else {
                print("snapshot FAILED \(name): scroll position did not change")
                window.orderOut(nil)
                return
            }
            print("snapshot scrolled \(name): \(previous) → \(scroll.contentView.bounds.origin.y)")
        }
        window.displayIfNeeded()
        capture(name, view: hosting, folder: folder)
        window.orderOut(nil)
    }

    static func capture(_ name: String, view: NSView, folder: URL) {
        let size = view.bounds.size
        let scale: CGFloat = 2
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ) else { return }
        rep.size = size
        view.cacheDisplay(in: view.bounds, to: rep)

        guard let data = rep.representation(using: .png, properties: [:]) else { return }
        let url = folder.appendingPathComponent("\(name).png")
        do {
            try data.write(to: url)
            print("snapshot \(url.path) \(rep.pixelsWide)x\(rep.pixelsHigh)")
        } catch {
            print("snapshot FAILED \(name): \(error)")
        }
    }

    private static func firstScrollView(in view: NSView) -> NSScrollView? {
        if let scroll = view as? NSScrollView { return scroll }
        return view.subviews.lazy.compactMap { firstScrollView(in: $0) }.first
    }
}

/// Borderless window parked far off-screen; AppKit must not pull it back on screen.
final class OffscreenWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    init(contentRect: CGRect) {
        super.init(contentRect: contentRect, styleMask: [.borderless], backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isReleasedWhenClosed = false
        animationBehavior = .none
    }

    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// Wallpaper + fake menu bar + glow + island, matching frame 1a of the design (720×390).
struct IslandScene: View {
    let store: UsageStore
    let settings: SettingsStore
    let open: Bool
    let light: Bool
    var alert: IslandAlert? = nil
    var showsAlertDetails = false

    private var panelHeight: CGFloat {
        if showsAlertDetails, let alert {
            let hosting = NSHostingView(rootView: IslandAlertDetailView(alert: alert, onOpen: {})
                .padding(.horizontal, 24).padding(.top, 54).padding(.bottom, 22)
                .frame(width: NotchController.alertDetailWidth).fixedSize(horizontal: false, vertical: true))
            return hosting.fittingSize.height
        }
        let hosting = NSHostingView(rootView: HoverPanelView(store: store, onOpenStats: {}, alert: alert)
            .frame(width: NotchController.expandedWidth).fixedSize(horizontal: false, vertical: true))
        return max(80, hosting.fittingSize.height)
    }

    var body: some View {
        let cameraWidth: CGFloat = alert == nil ? 380 : 216
        let closedHeight: CGFloat = alert == nil ? 44 : 38
        let islandSize = open ? CGSize(width: showsAlertDetails ? NotchController.alertDetailWidth : NotchController.expandedWidth, height: panelHeight)
            : alert != nil ? CGSize(width: cameraWidth + 2 * (NotchController.alertWingWidth + NotchController.alertSidePadding), height: closedHeight)
            : CGSize(width: cameraWidth, height: closedHeight)
        let radius: CGFloat = open ? NotchController.expandedRadius : 14
        let flare = open ? NotchGeometry.expandedTopRadius : NotchGeometry.collapsedTopRadius
        ZStack(alignment: .top) {
            LinearGradient(
                stops: [
                    .init(color: Color(hex: 0x5b6b8c), location: 0),
                    .init(color: Color(hex: 0x7c8aa6), location: 0.45),
                    .init(color: Color(hex: 0xb7a6a1), location: 1),
                ],
                startPoint: .top, endPoint: .bottom
            )
            HStack {
                HStack(spacing: 18) {
                    Text("Finder").fontWeight(.semibold)
                    Text(L10n.text("文件", "File"))
                }
                Spacer()
                HStack(spacing: 14) {
                    MenuBarIcon(stops: GlowGradient.stops(levels: store.levels, light: light), light: false)
                    Text("Wi‑Fi")
                    Text(L10n.text("周日 14:32", "Sun 14:32"))
                }
                .font(.ui(13))
            }
            .font(.ui(14))
            .foregroundStyle(Color.white)
            .padding(.horizontal, 18)
            .frame(height: 44)
            .background(Color(.sRGB, red: 20 / 255, green: 20 / 255, blue: 24 / 255, opacity: 0.55))

            GlowPreview(
                appearance: store.glowAppearance(light: light),
                settings: settings.settings,
                islandSize: islandSize,
                islandRadius: radius
            )
            .frame(width: 720, alignment: .top)
            IslandRootView(
                store: store, isOpen: open,
                collapsedSize: CGSize(width: cameraWidth + NotchGeometry.collapsedTopRadius * 2, height: closedHeight),
                collapsedTopRadius: NotchGeometry.collapsedTopRadius, collapsedBottomRadius: 14,
                lightBorder: light, onOpenStats: {}, alert: alert, showsAlertDetails: showsAlertDetails
            )
                .frame(width: islandSize.width + flare * 2, height: islandSize.height)
                .shadow(color: Color.black.opacity(0.35), radius: 15, y: 8)
        }
        .frame(width: 720, height: max(390, islandSize.height + 36))
        .clipShape(RoundedRectangle(cornerRadius: 14))
    }
}

/// 22×11 silhouette drawn in SwiftUI for snapshots (the real menu bar uses `StatusIconRenderer`).
struct MenuBarIcon: View {
    let stops: [GradientStop]
    let light: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: 4)
            .fill(LinearGradient(
                stops: stops.map { Gradient.Stop(color: Color($0.color), location: $0.location) },
                startPoint: .leading, endPoint: .trailing
            ))
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(light ? Color(hex: 0x1d1d1f) : Color.white, lineWidth: 1.5))
            .frame(width: 22, height: 11)
    }
}

/// Frame 1d's menu bar strip: icon + lowest remaining %.
struct MenuBarStrip: View {
    let store: UsageStore
    let light: Bool

    var body: some View {
        HStack(spacing: 16) {
            Spacer()
            HStack(spacing: 6) {
                MenuBarIcon(stops: GlowGradient.stops(levels: store.levels, light: light), light: light)
                Text(store.maxUsedPct.map(TokenFormat.percent) ?? "—").font(.tabular(13))
            }
            .padding(EdgeInsets(top: 3, leading: 7, bottom: 3, trailing: 7))
            .background(RoundedRectangle(cornerRadius: 5).fill(light ? Color.black.opacity(0.1) : Color.white.opacity(0.14)))
            Text("Wi‑Fi")
            Text(L10n.text("周日 14:32", "Sun 14:32"))
        }
        .font(.ui(13))
        .foregroundStyle(light ? Color(hex: 0x1d1d1f) : Color.white)
        .padding(.horizontal, 14)
        .frame(width: 340, height: 30)
        .background(light ? Color(hex: 0xe9e9ec) : Color(hex: 0x2c2c2e))
    }
}
