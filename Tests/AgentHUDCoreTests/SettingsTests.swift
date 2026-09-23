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
        XCTAssertTrue(s.launchAtLogin)
        XCTAssertTrue(s.showMenuBarIcon)
        XCTAssertEqual(s.appearance, .system)
        XCTAssertTrue(s.showIslandTokens)
        XCTAssertEqual(s.hoverDelay, 0.4, accuracy: 1e-9)
    }

    func testDecodesMissingAndRemovedFields() throws {
        let decoded = try JSONDecoder().decode(Settings.self, from: Data(#"{"glowRange":20,"glowPosition":"below","showIslandTrend":false,"pollInterval":300}"#.utf8))
        XCTAssertEqual(decoded.glowRange, 20)
        XCTAssertEqual(decoded.glowBlur, 8)
        XCTAssertTrue(decoded.glowOutwardOnly)
        XCTAssertEqual(decoded.appearance, .system)
        XCTAssertTrue(decoded.showIslandTokens)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: Data("{}".utf8)), Settings())
    }

    func testUnknownPreferencesAreIgnoredAndNeverEncoded() throws {
        // Deliberately wrong types prove unknown values are not decoded or validated.
        let settings = try JSONDecoder().decode(Settings.self, from: Data(#"{"notifyOnCompletion":"obsolete","mutedAlertVendors":42,"agentThresholds":false,"balanceWarningThresholds":false,"macNotificationsEnabled":[]}"#.utf8))
        XCTAssertEqual(settings, Settings())
        let agent = try JSONDecoder().decode(AgentDescriptor.self, from: Data(#"{"id":"a","vendor":"Claude","model":"5h","source":"","enabled":true,"connected":true,"warnPct":false,"critPct":false,"quotaThresholdOverride":false,"balanceThresholdOverride":false}"#.utf8))
        let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(agent)) as! [String: Any]
        XCTAssertEqual(Set(encoded.keys), ["id", "vendor", "model", "source", "enabled", "connected"])
    }

    func testRoundTrip() throws {
        let original = Settings().with {
            $0.appearance = .light
            $0.glowOutwardOnly = false
            $0.showIslandQuota = false; $0.showIslandTokens = false
            $0.showIslandSessions = false
        }
        let data = try JSONEncoder().encode(original)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: data), original)
    }

    func testGlowSizeBoundsWhenDecodingSettings() throws {
        for (range, blur, expectedRange, expectedBlur) in [
            // Above the bound and below it: both ends clamp, whatever the bound is set to.
            (Settings.glowSizeRange.upperBound * 2, Settings.glowSizeRange.upperBound + 10,
             Settings.glowSizeRange.upperBound, Settings.glowSizeRange.upperBound),
            (-1, -1, 0, 0),
            (0, Settings.glowSizeRange.upperBound, 0, Settings.glowSizeRange.upperBound),
            (Settings.glowSizeRange.upperBound, 0, Settings.glowSizeRange.upperBound, 0),
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

final class AgentSettingsTests: XCTestCase {
    func testAccountDetailsUseObservedPlansAndAPIProvidersWithoutGuessing() {
        let now = Date()
        let sources = [SourceStatus(id: "cursor", name: "Cursor", detail: "technical details", state: .installed),
                       SourceStatus(id: "deepseek", name: "DeepSeek", detail: "", state: .notDetected)]
        let report = UsageReport(generatedAt: now, snapshots: [], sessions: [], subscriptions: ["kimi-plan": "Allegretto"], services: [
                .init(client: "OpenCode", provider: "Anthropic", product: .api),
                .init(client: "OpenCode", provider: "OpenAI", product: .api),
                .init(client: "OpenCode", provider: "Anthropic", product: .api),
                .init(client: "OpenCode", provider: "Kimi", product: .plan, accountID: "kimi-plan"),
                .init(client: "Pi", provider: "GLM", product: .plan),
            ])
        let groups = AgentSettingsGroup.make(sources: sources, agents: [], report: report)
        XCTAssertEqual(groups.first { $0.id == "OpenCode" }?.apiProviders, ["Anthropic", "OpenAI"])
        XCTAssertEqual(groups.first { $0.id == "OpenCode" }?.plans, ["Kimi · Allegretto"])
        for id in ["Cursor", "DeepSeek", "Pi"] {
            XCTAssertEqual(groups.first { $0.id == id }?.plans, [])
            XCTAssertEqual(groups.first { $0.id == id }?.apiProviders, [])
        }
    }

    func testAPIBillingBelongsToProviderAndOnlyIdenticalAccountsMerge() {
        func pool(_ scope: String) -> BillingPool {
            .init(provider: "Anthropic", realm: "Global", product: .api, scope: scope, evidence: .account, entitlement: "api")
        }
        let shared = pool("shared"), other = pool("other")
        let agents = [AgentDescriptor(id: "open", vendor: "OpenCode", model: "Model A", source: "", enabled: true, billingPool: shared),
                      AgentDescriptor(id: "pi", vendor: "Pi", model: "Model B", source: "", enabled: true, billingPool: shared)]
        let groups = AgentSettingsGroup.make(sources: [], agents: agents)
        XCTAssertEqual(groups.map(\.id), ["Anthropic", "OpenCode", "Pi"])
        XCTAssertEqual(groups.first?.apiProviders, ["Anthropic"])
        XCTAssertEqual(groups.first { $0.id == "OpenCode" }?.apiProviders, ["Anthropic"])
        XCTAssertEqual(groups.first { $0.id == "Pi" }?.apiProviders, ["Anthropic"])
        XCTAssertTrue(groups.first { $0.id == "Pi" }!.agents.isEmpty)
        func billing(_ client: String, _ account: BillingPool, _ at: Double, _ amount: Decimal) -> APIBilling {
            .init(vendor: client, balances: [.init(currency: "USD", total: amount, granted: 0, toppedUp: amount)],
                  isAvailable: true, updatedAt: Date(timeIntervalSince1970: at), costs: [], notice: nil, billingPool: account)
        }
        let merged = CombinedUsageProvider.mergeBilling([billing("OpenCode", shared, 1, 10), billing("Pi", shared, 2, 8),
                                                         billing("Pi", other, 3, 20)])
        XCTAssertEqual(merged.count, 2)
        XCTAssertEqual(Set(merged.map(\.vendor)), ["Anthropic"])
        XCTAssertEqual(merged.first { $0.billingPool == shared }?.balances.first?.total, 8)
        XCTAssertTrue(merged.first { $0.billingPool == shared }!.contains(agents[1]))
        XCTAssertFalse(merged.first { $0.billingPool == other }!.contains(agents[1]))
    }

    func testServiceDetailsSurviveOldCacheAndFailedReadings() throws {
        let report = UsageReport(generatedAt: Date(), snapshots: [], sessions: [], services: [.init(client: "Pi", provider: "OpenAI", product: .api)])
        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(report)) as? [String: Any])
        legacy.removeValue(forKey: "services")
        let empty = try JSONDecoder().decode(UsageReport.self, from: JSONSerialization.data(withJSONObject: legacy))
        XCTAssertNil(empty.services)
        let retained = empty.retainingReadings(from: report)
        XCTAssertEqual(retained.services, report.services)
        XCTAssertEqual(try JSONDecoder().decode(UsageReport.self, from: JSONEncoder().encode(retained)).services, report.services)
    }

    func testGroupsIncludeSourcesWithoutWindowsAndPreserveWindowOrder() {
        let sources = [
            SourceStatus(id: "claude-code", name: "Claude", detail: "", state: .ready(plan: "max_20x")),
            SourceStatus(id: "cursor", name: "Cursor", detail: "", state: .notDetected),
            SourceStatus(id: "codex-cli", name: "Codex", detail: "", state: .installed),
            SourceStatus(id: "chatgpt", name: "ChatGPT 聊天额度", detail: "", state: .needsAuthorization),
        ]
        let agents = [
            AgentDescriptor(id: "cx", vendor: "Codex", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "c1", vendor: "Claude", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "c2", vendor: "Claude", model: "Weekly", source: "", enabled: false),
            AgentDescriptor(id: "chatgpt", vendor: "ChatGPT", model: "Plus", source: "", enabled: false),
        ]
        let groups = AgentSettingsGroup.make(sources: sources, agents: agents)
        XCTAssertEqual(groups.map(\.id), ["Codex", "Claude", "ChatGPT", "Cursor"])
        XCTAssertEqual(groups[1].agents.map(\.id), ["c1", "c2"])
        XCTAssertEqual(groups[1].displayedCount, 1)
        XCTAssertEqual(groups[1].agents.count, 2)
        XCTAssertEqual(groups[2].source?.id, "chatgpt")
        XCTAssertEqual(groups[3].displayedCount, 0)
        XCTAssertTrue(groups[3].agents.isEmpty)
        let hidden = AgentSettingsGroup.make(sources: sources, agents: agents.map { $0.with(enabled: false) })
        XCTAssertEqual(hidden.map(\.displayedCount), [0, 0, 0, 0])
        XCTAssertEqual(hidden[1].agents.count, 2)
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
        XCTAssertEqual(store.maxUsedPct, 93)
        XCTAssertTrue(store.hasLiveSession)
    }

    func testAPIBillingReplacesQuotaRowAndFollowsAgentToggle() throws {
        let store = makeStore()
        let deepseek = try XCTUnwrap(DefaultAgents.list.first { $0.id == "deepseek" }).with(enabled: true)
        let claude = DemoData.agents[0]
        store.settings.updateAgents { _ in [deepseek, claude] }
        XCTAssertEqual(store.rows.map(\.id), [claude.id], "API agents never create a quota placeholder, even before the first fetch")
        XCTAssertNil(store.rows[0].remainingPct, "subscription agents retain their pending quota row")

        let billing = DemoData.deepSeekBilling(now: Date())
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [],
                                         billing: [billing]))
        XCTAssertEqual(store.rowGroups.map(\.vendor), ["Claude"])
        XCTAssertEqual(store.enabledBilling, [billing])

        store.settings.setAgent(id: "deepseek", enabled: false)
        XCTAssertTrue(store.enabledBilling.isEmpty)
        XCTAssertEqual(store.report?.billing, [billing], "display switches preserve the underlying billing data")
        store.settings.setAgent(id: "deepseek", enabled: true)
        XCTAssertEqual(store.enabledBilling, [billing], "the card returns without waiting for a refresh")
        XCTAssertEqual(store.rows.map(\.id), [claude.id])

        let unavailable = APIBilling(vendor: "DeepSeek", balances: [], isAvailable: nil, updatedAt: nil, costs: [], notice: "offline")
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [],
                                         billing: [unavailable]))
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
        let usage = [("claude-opus", 100), ("codex", 200), ("antigravity", 900)].map { id, tokens in
            UsageBucket(start: hour, agentId: id, tokensIn: tokens, tokensOut: 0)
        }
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [],
            consumers: DemoData.agents, usage: usage))

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

    func testQuotaForecastUsesOnlyTheHoveredWindowsInsights() throws {
        let store = makeStore()
        let now = Date()
        let snapshots = ["claude-opus", "claude-sonnet", "codex"].map {
            UsageSnapshot(agentId: $0, remainingPct: 20, resetAt: now.addingTimeInterval(5 * 3600), windowDuration: 5 * 3600, updatedAt: now)
        }
        func insights(hours: Double) -> UsageInsights {
            UsageInsights(burnRatePctPerHour: 20 / hours, timeToExhaust: hours * 3600, weeklyCapHits: 0,
                          weeklyWaitTotal: 0, weeklyWaitLongest: 0, weeklyWaitLongestAt: nil)
        }
        store.replace(report: UsageReport(generatedAt: now, snapshots: snapshots, sessions: [],
            insightsByAgent: ["claude-opus": insights(hours: 1), "codex": insights(hours: 2.5)]))
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

    func testQuotaTokenRateUsesOnlyMappedConsumersInTheObservedCurrentCycle() throws {
        let store = makeStore()
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let snapshot = UsageSnapshot(agentId: "claude-weekly", remainingPct: 60,
                                     resetAt: now.addingTimeInterval(2 * 3600), windowDuration: 5 * 3600,
                                     updatedAt: now)
        store.replace(report: UsageReport(generatedAt: now, snapshots: [snapshot], sessions: [], usage: [
            UsageBucket(start: now.addingTimeInterval(-2 * 3600), agentId: "claude-model:opus", tokensIn: 24_000, tokensOut: 6_000),
            UsageBucket(start: now.addingTimeInterval(-3600), agentId: "claude-model:sonnet", tokensIn: 8_000, tokensOut: 2_000),
            UsageBucket(start: now.addingTimeInterval(-1800), agentId: "codex-model:gpt", tokensIn: 100_000, tokensOut: 0),
            UsageBucket(start: now.addingTimeInterval(900), agentId: "claude-model:opus", tokensIn: 50_000, tokensOut: 0),
        ], consumerIdsByQuota: ["claude-weekly": ["claude-model:opus", "claude-model:sonnet"]]))
        store.now = now

        XCTAssertEqual(try XCTUnwrap(store.quotaTokensPerHour(for: "claude-weekly")), 20_000,
                       "40k mapped tokens over the two locally observed hours")
        XCTAssertNil(store.quotaTokensPerHour(for: "missing"))
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
        store.replace(report: UsageReport(generatedAt: now, snapshots: [], sessions: sessions,
            consumers: consumers,
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
        ]))
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
