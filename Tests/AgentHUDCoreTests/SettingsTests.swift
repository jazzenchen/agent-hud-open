import XCTest
@testable import AgentHUDCore

final class SettingsTests: XCTestCase {
    func testDefaultsMatchDesign() {
        let s = Settings()
        XCTAssertEqual(s.breathSeconds, 3)
        XCTAssertEqual(s.breathAmplitude, 0.6)
        XCTAssertEqual(s.glowRange, 14)
        XCTAssertEqual(s.glowBlur, 8)
        XCTAssertTrue(s.glowOutwardOnly)
        XCTAssertEqual(s.glowBrightness, 0.9)
        XCTAssertEqual(s.hoverDelayMs, 400)
        XCTAssertEqual(s.collapseDelayMs, 200)
        XCTAssertEqual(s.pollInterval, .oneMinute)
        XCTAssertTrue(s.launchAtLogin)
        XCTAssertTrue(s.showMenuBarIcon)
        XCTAssertEqual(s.appearance, .system)
        XCTAssertTrue(s.showIslandTokens)
        XCTAssertEqual(s.hoverDelay, 0.4, accuracy: 1e-9)
    }

    func testDecodesMissingAndRemovedFields() throws {
        let decoded = try JSONDecoder().decode(Settings.self, from: Data(#"{"glowRange":20,"glowPosition":"below","showIslandTrend":false}"#.utf8))
        XCTAssertEqual(decoded.glowRange, 20)
        XCTAssertEqual(decoded.glowBlur, 8)
        XCTAssertTrue(decoded.glowOutwardOnly)
        XCTAssertEqual(decoded.appearance, .system)
        XCTAssertTrue(decoded.showIslandTokens)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: Data("{}".utf8)), Settings())
    }

    func testRoundTrip() throws {
        let original = Settings().with {
            $0.appearance = .light; $0.pollInterval = .fiveMinutes
            $0.glowOutwardOnly = false
            $0.showIslandQuota = false; $0.showIslandTokens = false
            $0.showIslandSessions = false
        }
        let data = try JSONEncoder().encode(original)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: data), original)
    }

    func testGlowSizeBoundsWhenDecodingSettings() throws {
        for (range, blur, expectedRange, expectedBlur) in [
            (40.0, 30.0, 20.0, 20.0),
            (-1, -1, 0, 0),
            (0, 20, 0, 20),
            (20, 0, 20, 0),
        ] {
            let data = try JSONEncoder().encode(["glowRange": range, "glowBlur": blur])
            let decoded = try JSONDecoder().decode(Settings.self, from: data)
            XCTAssertEqual(decoded.glowRange, expectedRange)
            XCTAssertEqual(decoded.glowBlur, expectedBlur)
            XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(decoded)), decoded)
        }
    }

    func testWithReturnsCopy() {
        let a = Settings()
        let b = a.with { $0.glowRange = 20 }
        XCTAssertEqual(a.glowRange, 14)
        XCTAssertEqual(b.glowRange, 20)
    }
}

final class AgentDescriptorTests: XCTestCase {
    func testGroupsKeepModelOrderAndCombineTheSameDisplayedAgent() {
        let models = DemoData.agents
        let interleaved = [models[0], models[3], models[1], models[4], models[2], models[5]]
        XCTAssertEqual(interleaved.agentGroups.map(\.id), ["Claude", "Codex", "Antigravity", "ChatGPT", "DeepSeek"])
        XCTAssertEqual(interleaved.groupedAgentOrder, [models[0], models[1], models[3], models[4], models[2], models[5]])
    }

    func testMovingGroupsInBothDirectionsKeepsModelsTogether() {
        let models = DemoData.agents
        let moved = models.movingGroup(id: "Claude", to: "Antigravity")
        XCTAssertEqual(moved, [models[2], models[3], models[4], models[0], models[1], models[5]])
        XCTAssertEqual(moved.movingGroup(id: "Claude", to: "ChatGPT"), models)
    }

    func testMovingReorders() {
        let ids = DemoData.agents.moving(id: "codex", to: 0).map(\.id)
        XCTAssertEqual(ids.first, "codex")
        XCTAssertEqual(ids.count, DemoData.agents.count)
        XCTAssertEqual(DemoData.agents.moving(id: "nope", to: 0), DemoData.agents)
        XCTAssertEqual(DemoData.agents.moving(id: "codex", to: 99), DemoData.agents)
    }

    func testWithKeepsOtherFields() {
        let a = DemoData.agents[0].with(enabled: false)
        XCTAssertFalse(a.enabled)
        XCTAssertEqual(a.source, DemoData.agents[0].source)
        XCTAssertEqual(a.displayName, "Claude · Opus 4.5")
    }
}

@MainActor
final class SettingsStoreTests: XCTestCase {
    private func makeDefaults() -> UserDefaults {
        let suite = "AgentHUDTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        return defaults
    }

    func testPersistsSettingsAndAgents() {
        let defaults = makeDefaults()
        let store = SettingsStore(defaults: defaults, defaultAgents: DemoData.agents)
        store.update {
            $0.glowRange = 18
            $0.glowOutwardOnly = false
            $0.showIslandTokens = false
        }
        store.setAgent(id: "chatgpt", enabled: false)
        store.moveAgent(id: "codex", to: 0)
        store.markOnboardingComplete()

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.settings.glowRange, 18)
        XCTAssertFalse(reloaded.settings.glowOutwardOnly)
        XCTAssertFalse(reloaded.settings.showIslandTokens)
        XCTAssertEqual(reloaded.agents.first?.id, "codex")
        XCTAssertFalse(reloaded.agents.first { $0.id == "chatgpt" }!.enabled)
        XCTAssertTrue(reloaded.hasCompletedOnboarding)
        XCTAssertEqual(reloaded.enabledAgents.count, 3)
    }

    func testFreshStoreUsesDefaults() {
        let store = SettingsStore(defaults: makeDefaults())
        XCTAssertEqual(store.settings, Settings())
        XCTAssertEqual(store.agents, DefaultAgents.list)
        XCTAssertEqual(store.enabledAgents.map(\.id), ["codex"], "Claude rows are discovered from local data")
        XCTAssertFalse(store.hasCompletedOnboarding)
    }

    func testGroupedOrderPersistsWithModelToggles() throws {
        let defaults = makeDefaults()
        let models = DemoData.agents
        let interleaved = [models[0], models[2], models[1], models[3], models[4], models[5]]
        defaults.set(try JSONEncoder().encode(interleaved), forKey: SettingsStore.Keys.agents)
        let store = SettingsStore(defaults: defaults)
        XCTAssertEqual(store.agents, models, "previously interleaved rows become contiguous groups")

        store.setAgent(id: "claude-sonnet", enabled: false)
        store.moveAgentGroup(id: "Codex", to: "Claude")
        store.moveAgent(id: "claude-sonnet", to: 1)

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.agents.map(\.id), ["codex", "claude-sonnet", "claude-opus", "chatgpt", "antigravity", "deepseek"])
        XCTAssertEqual(reloaded.agents[1], models[1].with(enabled: false))
        XCTAssertEqual(reloaded.enabledAgents.map(\.id), ["codex", "claude-opus", "chatgpt"])
    }

    func testMergeDiscoveredInsertsByVendorAndUpdatesNames() {
        let store = SettingsStore(defaults: makeDefaults())
        let fable = AgentDescriptor(id: "claude-fable", vendor: "Claude", model: "Fable 5.1", source: "Claude Code", enabled: true)
        let sonnet = AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 5", source: "Claude Code", enabled: true)
        store.mergeDiscovered([fable, sonnet])
        XCTAssertEqual(store.agents.map(\.id).prefix(3), ["claude-fable", "claude-sonnet", "codex"], "new vendor goes to the top, in discovery order")

        store.setAgent(id: "claude-sonnet", enabled: false)
        store.moveAgent(id: "claude-sonnet", to: 0)
        let haiku = AgentDescriptor(id: "claude-haiku", vendor: "Claude", model: "Haiku 4.5", source: "Claude Code", enabled: false)
        store.mergeDiscovered([AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 5.1", source: "Claude Code", enabled: true), haiku])
        XCTAssertEqual(store.agents.map(\.id).prefix(4), ["claude-sonnet", "claude-fable", "claude-haiku", "codex"], "existing order kept, new family after the last Claude row")
        let updated = store.agents.first { $0.id == "claude-sonnet" }!
        XCTAssertEqual(updated.model, "Sonnet 5.1", "display name follows the newest version seen")
        XCTAssertFalse(updated.enabled, "user toggles survive")
    }
}

@MainActor
final class UsageStoreTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func makeStore() -> UsageStore {
        let suite = "AgentHUDTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        return UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: DemoData.agents))
    }

    func testAgentsWithoutDataStayOutOfGlow() async {
        let suite = "AgentHUDTests.\(UUID().uuidString)"
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: UserDefaults(suiteName: suite)!))
        XCTAssertEqual(store.levels, [])
        XCTAssertTrue(store.isLoading)
        await store.refresh()
        XCTAssertFalse(store.isLoading)
        XCTAssertEqual(store.rows.map(\.id), ["codex"])
        XCTAssertEqual(store.levels.count, 1, "demo data covers the default codex row")
    }

    func testInitialLoadingStopsOnFailureOrPause() async {
        struct FailingProvider: UsageProvider {
            func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
                throw UsageProviderError("offline")
            }
        }
        let store = UsageStore(provider: FailingProvider(), settings: makeStore().settings)
        XCTAssertTrue(store.isLoading)
        await store.refresh()
        XCTAssertFalse(store.isLoading)
        XCTAssertEqual(store.lastError, "offline")

        let paused = makeStore()
        paused.pause(for: 3600)
        XCTAssertFalse(paused.isLoading)
    }

    func testRowsFollowAgentOrderAndThresholds() async {
        let store = makeStore()
        await store.refresh()
        XCTAssertEqual(store.rows.map(\.id), ["claude-opus", "claude-sonnet", "chatgpt", "codex"])
        XCTAssertEqual(store.rows.map(\.level), [.ok, .warning, .ok, .critical])
        XCTAssertEqual(store.minRemainingPct, 7)
        XCTAssertTrue(store.hasLiveSession)
        XCTAssertEqual(store.weeklyByVendor.map(\.vendor), ["Claude", "ChatGPT"])
        XCTAssertEqual(store.history(for: "claude-opus", lastHours: 24).count, 24)
    }

    func testAPIBillingReplacesQuotaRowAndFollowsAgentToggle() throws {
        let store = makeStore()
        let deepseek = try XCTUnwrap(DefaultAgents.list.first { $0.id == "deepseek" }).with(enabled: true)
        let claude = DemoData.agents[0]
        store.settings.updateAgents { _ in [deepseek, claude] }
        XCTAssertEqual(store.rows.map(\.id), [claude.id], "API agents never create a quota placeholder, even before the first fetch")
        XCTAssertNil(store.rows[0].remainingPct, "subscription agents retain their pending quota row")

        let billing = DemoData.deepSeekBilling(now: Date())
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [], history: [],
                                         activity: .empty, insights: .empty, billing: [billing]))
        XCTAssertEqual(store.rowGroups.map(\.vendor), ["Claude"])
        XCTAssertEqual(store.enabledBilling, [billing])

        store.settings.setAgent(id: "deepseek", enabled: false)
        XCTAssertTrue(store.enabledBilling.isEmpty)
        XCTAssertEqual(store.report?.billing, [billing], "display switches preserve the underlying billing data")
        store.settings.setAgent(id: "deepseek", enabled: true)
        XCTAssertEqual(store.enabledBilling, [billing], "the card returns without waiting for a refresh")
        XCTAssertEqual(store.rows.map(\.id), [claude.id])

        let unavailable = APIBilling(vendor: "DeepSeek", balances: [], isAvailable: nil, updatedAt: nil, costs: [], notice: "offline")
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [], history: [],
                                         activity: .empty, insights: .empty, billing: [unavailable]))
        XCTAssertEqual(store.enabledBilling, [unavailable], "a balance error stays in the cost card")
        XCTAssertEqual(store.rows.map(\.id), [claude.id])
    }

    func testGlowFollowsPauseAndHide() async {
        let store = makeStore()
        await store.refresh()
        XCTAssertTrue(store.glowAppearance(light: false).breathing)
        store.pause(for: 3600)
        XCTAssertTrue(store.isPaused)
        XCTAssertFalse(store.glowAppearance(light: false).breathing)
        store.resume()
        XCTAssertFalse(store.isPaused)
        store.glowHidden = true
        XCTAssertTrue(store.glowAppearance(light: false).hidden)
    }

    func testTokenChartsIncludeAllModelsRegardlessOfQuotaSwitches() {
        let store = makeStore()
        let hour = Calendar.current.dateInterval(of: .hour, for: Date())!.start
        let consumption = [("claude-opus", 100), ("codex", 200), ("antigravity", 900)].map { id, tokens in
            UsageEvent(timestamp: hour, agentId: id, tokensIn: tokens, tokensOut: 0)
        }
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [], history: [],
            activity: .empty, insights: .empty, consumers: DemoData.agents, consumption: consumption))

        func total() -> Int { store.tokenColumns.reduce(0) { $0 + $1.total } }
        XCTAssertEqual(total(), 1200, "quota settings do not exclude token spenders")
        store.settings.setAgent(id: "claude-opus", enabled: false)
        XCTAssertEqual(total(), 1200)
        store.settings.setAgent(id: "codex", enabled: false)
        XCTAssertEqual(total(), 1200)
        for size in TokenBucketSize.allCases {
            store.tokenBucketSize = size
            XCTAssertEqual(store.statsRange, .hours24)
            XCTAssertEqual(total(), 1200)
        }
    }

    func testSelectedWindowUsesItsOwnInsightsAndFallsBackWhenDisabled() {
        let store = makeStore()
        let report = UsageReport(generatedAt: Date(), snapshots: [], sessions: [], history: [], activity: .empty,
                                 insights: .empty, insightsByAgent: [
                                    "claude-opus": UsageInsights(burnRatePctPerHour: 5, timeToExhaust: nil, weeklyCapHits: 1,
                                                               weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                                               weeklyShare: [:], windowSessionCount: 1, windowUsedPct: 30),
                                    "codex": UsageInsights(burnRatePctPerHour: 2, timeToExhaust: nil, weeklyCapHits: 0,
                                                         weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                                                         weeklyShare: [:], windowSessionCount: 1, windowUsedPct: 7)
                                 ])
        store.replace(report: report)
        store.selectedQuotaId = "codex"
        XCTAssertEqual(store.primaryInsights.burnRatePctPerHour, 2)
        store.settings.setAgent(id: "codex", enabled: false)
        XCTAssertEqual(store.primaryRow?.id, "claude-opus")
        XCTAssertEqual(store.primaryInsights.burnRatePctPerHour, 5)
    }

    func testQuotaForecastUsesOnlyTheHoveredWindowsInsights() throws {
        let store = makeStore()
        let now = Date()
        let snapshots = ["claude-opus", "claude-sonnet", "codex"].map {
            UsageSnapshot(agentId: $0, remainingPct: 20, resetAt: now.addingTimeInterval(5 * 3600), windowDuration: 5 * 3600, updatedAt: now)
        }
        func insights(hours: Double) -> UsageInsights {
            UsageInsights(burnRatePctPerHour: 20 / hours, timeToExhaust: hours * 3600, weeklyCapHits: 0,
                          weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil,
                          weeklyShare: [:], windowSessionCount: 0, windowUsedPct: 80)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: snapshots, sessions: [], history: [], activity: .empty,
            insights: insights(hours: 4), insightsByAgent: ["claude-opus": insights(hours: 1), "codex": insights(hours: 2.5)]))
        store.selectedQuotaId = "codex"
        XCTAssertEqual(try XCTUnwrap(store.quotaForecastHint(for: "claude-opus")), "耗尽 ~1小时")
        XCTAssertEqual(try XCTUnwrap(store.quotaForecastHint(for: "codex")), "耗尽 ~2小时30分")
        XCTAssertEqual(try XCTUnwrap(store.quotaForecastHint(for: "claude-sonnet")), "记录不足",
                      "a row without its own samples must not use the selected row or global forecast")
        XCTAssertNil(store.quotaForecastHint(for: "missing"))
        store.settings.update { $0.showResetCountdown = false }
        XCTAssertEqual(try XCTUnwrap(store.quotaForecastHint(for: "codex")), "耗尽 ~2小时30分",
                      "hiding the reset column does not hide the hover forecast")
    }

    func testDisablingAgentDropsItFromGlow() async {
        let store = makeStore()
        await store.refresh()
        store.settings.setAgent(id: "codex", enabled: false)
        XCTAssertEqual(store.levels, [.ok, .warning, .ok])
    }

    func testQuotaSettingsDoNotGateConsumersOrSessions() {
        let store = makeStore()
        let now = Date()
        let opus = AgentDescriptor(id: "claude-opus", vendor: "Claude", model: "Opus", source: "", enabled: true)
        let sonnet = AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet", source: "", enabled: true)
        let codex = AgentDescriptor(id: "codex-model:gpt", vendor: "Codex", model: "GPT", source: "", enabled: true)
        store.settings.updateAgents { _ in [
            AgentDescriptor(id: "claude-session", vendor: "Claude", model: "5h", source: "", enabled: false),
            AgentDescriptor(id: "claude-weekly-opus", vendor: "Claude", model: "Opus", source: "", enabled: true),
            AgentDescriptor(id: "codex-weekly", vendor: "Codex", model: "Weekly", source: "", enabled: false),
        ] }
        let consumers = [opus, sonnet, codex]
        let sessions = consumers.map {
            LiveSession(id: $0.id, agentId: $0.id, task: $0.model, terminal: nil,
                        startedAt: now.addingTimeInterval(-60), pctOfWindow: nil, tokensIn: 10, tokensOut: 20)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: sessions, history: [],
            activity: .empty, insights: .empty, consumers: consumers,
            subscriptions: ["Claude": "max", "Codex": "pro"], consumerIdsByQuota: [
                "claude-session": [opus.id, sonnet.id], "claude-weekly-opus": [opus.id], "codex-weekly": [codex.id],
            ]))
        XCTAssertEqual(store.consumers.map(\.id), consumers.map(\.id))
        XCTAssertEqual(store.statsSessions.map(\.agentId), consumers.map(\.id))
        XCTAssertEqual(store.subscriptions, ["Claude": "max"])

        store.settings.setAgent(id: "codex-weekly", enabled: true)
        XCTAssertEqual(store.statsSessions.map(\.agentId), consumers.map(\.id))
        store.settings.setAgent(id: "claude-weekly-opus", enabled: false)
        XCTAssertEqual(store.consumers.map(\.id), consumers.map(\.id))
        XCTAssertEqual(store.subscriptions, ["Codex": "pro"])
        store.settings.setAgent(id: "claude-session", enabled: true)
        XCTAssertEqual(store.consumers.map(\.id), consumers.map(\.id), "a shared quota includes each model it covers")

        store.settings.updateAgents { $0.map { $0.with(enabled: false) } }
        XCTAssertEqual(store.sessions.count, 3)
        XCTAssertEqual(store.consumers.count, 3)
        XCTAssertTrue(store.subscriptions.isEmpty)
        XCTAssertNil(store.primaryRow)
    }

    func testActiveSessionsFollowSelectedRangeIncludingLongRunningSessions() {
        let store = makeStore()
        let now = Date()
        func session(_ id: String, startedHoursAgo: Double, endedHoursAgo: Double? = nil) -> LiveSession {
            LiveSession(id: id, agentId: "codex", task: id, terminal: nil,
                        startedAt: now.addingTimeInterval(-startedHoursAgo * 3600),
                        endedAt: endedHoursAgo.map { now.addingTimeInterval(-$0 * 3600) },
                        pctOfWindow: nil, tokensIn: 0, tokensOut: 0, observedAt: now)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: [
            session("running", startedHoursAgo: 240),
            session("recent", startedHoursAgo: 10, endedHoursAgo: 2),
            session("today", startedHoursAgo: 20, endedHoursAgo: 8),
            session("week", startedHoursAgo: 96, endedHoursAgo: 72),
            session("older", startedHoursAgo: 240, endedHoursAgo: 200),
            session("future", startedHoursAgo: -24),
        ], history: [], activity: .empty, insights: .empty))
        XCTAssertEqual(store.statsRange, .hours24)
        XCTAssertEqual(store.statsSessions.map(\.id), ["running", "recent", "today"])
        store.setStatsRange(.hours5)
        XCTAssertEqual(store.statsSessions.map(\.id), ["running", "recent"])
        store.setStatsRange(.hours24)
        XCTAssertEqual(store.statsSessions.map(\.id), ["running", "recent", "today"])
        store.setStatsRange(.days7)
        XCTAssertEqual(store.statsSessions.map(\.id), ["running", "recent", "today", "week"])
        store.setStatsRange(.hours5)
        XCTAssertEqual(store.statsSessions.map(\.id), ["running", "recent"])
    }
}
