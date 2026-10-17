import XCTest
@testable import AgentHUDCore

/// Six rows of five vendors for tests about lists of rows and their switches: two Claude models, ChatGPT, Codex, and
/// Antigravity and DeepSeek switched off.
private let sixRows: [AgentDescriptor] = [
    AgentDescriptor(id: "claude-opus", vendor: "Claude", model: "Opus 5.5", source: L10n.sourceClaudeSessions, enabled: true),
    AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 5.5", source: L10n.sourceClaudeSessions, enabled: true),
    AgentDescriptor(id: "chatgpt", vendor: "ChatGPT", model: "Plus", source: L10n.sourceBrowserAuth, enabled: true),
    AgentDescriptor(id: "codex", vendor: "Codex", model: "CLI", source: L10n.sourceCodexAppServer, enabled: true),
    AgentDescriptor(id: "antigravity", vendor: "Antigravity", model: "Agent", source: L10n.sourceNotConnected, enabled: false),
    AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceNotConnected, enabled: false),
]

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
        XCTAssertFalse(s.openImmediatelyAtTop)
        XCTAssertEqual(s.collapseDelayMs, 200)
        XCTAssertTrue(s.launchAtLogin)
        XCTAssertTrue(s.showMenuBarIcon)
        XCTAssertEqual(s.appearance, .system)
        XCTAssertEqual(s.hoverDelay, 0.4, accuracy: 1e-9)
    }

    func testDecodesMissingAndRemovedFields() throws {
        let decoded = try JSONDecoder().decode(Settings.self, from: Data(#"{"glowRange":20,"glowPosition":"below","showIslandTrend":false,"pollInterval":300}"#.utf8))
        XCTAssertEqual(decoded.glowRange, 20)
        XCTAssertEqual(decoded.glowBlur, 8)
        XCTAssertTrue(decoded.glowOutwardOnly)
        XCTAssertEqual(decoded.appearance, .system)
        XCTAssertFalse(decoded.openImmediatelyAtTop)
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

    /// Agents may arrange the HUD only once a person allows it: off by default, off for settings saved before the
    /// switch existed, and kept as set.
    func testHUDToolsStartOffAndKeepTheirSwitch() throws {
        XCTAssertFalse(Settings().hudTools)
        XCTAssertFalse(try JSONDecoder().decode(Settings.self, from: Data(#"{"clientHooks":true}"#.utf8)).hudTools)
        let on = Settings().with { $0.hudTools = true }
        let data = try JSONEncoder().encode(on)
        let stored = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(stored["hudTools"] as? Bool, true, "stored under its own key")
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: data), on)
        XCTAssertFalse(try JSONDecoder().decode(Settings.self, from: JSONEncoder().encode(Settings())).hudTools)
    }

    func testRoundTrip() throws {
        let original = Settings().with {
            $0.appearance = .light
            $0.glowOutwardOnly = false
            $0.botLogos = false
            $0.openImmediatelyAtTop = true
        }
        let data = try JSONEncoder().encode(original)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: data), original)
    }

    // MARK: The app's appearance

    /// Match screen is a switch beside the appearance, saved under a key of its own: a build that has no such switch
    /// reads the appearance as it is, and this one reads both back.
    func testMatchScreenIsASwitchBesideTheAppearance() throws {
        for mode in AppearanceMode.allCases {
            for follows in [false, true] {
                let settings = Settings().with { $0.appearance = mode; $0.appearanceFollowsScreen = follows }
                let data = try JSONEncoder().encode(settings)
                let stored = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
                XCTAssertEqual(stored["appearance"] as? String, mode.rawValue, "what an older build reads")
                XCTAssertEqual(stored["appearanceFollowsScreen"] as? Bool, follows)
                let read = try JSONDecoder().decode(Settings.self, from: data)
                XCTAssertEqual(read.appearance, mode)
                XCTAssertEqual(read.appearanceFollowsScreen, follows)
            }
        }
        XCTAssertEqual(AppearanceMode.allCases, [.system, .dark, .light], "the appearance itself keeps its three")
    }

    /// Settings saved before the switch read as they were saved, with Match screen off; the earlier way of saving Match
    /// screen, System beside the key, reads as System with the switch on.
    func testSettingsWithoutMatchScreenReadAsSaved() throws {
        func read(_ json: String) throws -> (AppearanceMode, Bool) {
            let settings = try JSONDecoder().decode(Settings.self, from: Data(json.utf8))
            return (settings.appearance, settings.appearanceFollowsScreen)
        }
        XCTAssertTrue(try read(#"{"appearance":"dark"}"#) == (.dark, false))
        XCTAssertTrue(try read(#"{"appearance":"light"}"#) == (.light, false))
        XCTAssertTrue(try read(#"{}"#) == (.system, false))
        XCTAssertTrue(try read(#"{"appearance":"system","appearanceFollowsScreen":true}"#) == (.system, true))
        XCTAssertTrue(try read(#"{"appearance":"dark","appearanceFollowsScreen":true}"#) == (.dark, true))
    }

    /// An appearance this build does not know, saved by a newer one, or a value of the wrong type, reads as the default
    /// and costs no other setting; so does a follow-screen key of the wrong type.
    func testAnUnknownAppearanceCostsNoOtherSetting() throws {
        let newer = try JSONDecoder().decode(Settings.self, from: Data(
            #"{"appearance":"sepia","glowRange":20,"launchAtLogin":false}"#.utf8))
        XCTAssertEqual(newer.appearance, .system)
        XCTAssertEqual(newer.glowRange, 20)
        XCTAssertFalse(newer.launchAtLogin)
        XCTAssertEqual(try JSONDecoder().decode(Settings.self, from: Data(#"{"appearance":7,"glowRange":20}"#.utf8)),
                       Settings().with { $0.glowRange = 20 })
        let mistyped = try JSONDecoder().decode(Settings.self, from: Data(
            #"{"appearance":"light","appearanceFollowsScreen":"yes"}"#.utf8))
        XCTAssertEqual(mistyped.appearance, .light)
        XCTAssertFalse(mistyped.appearanceFollowsScreen)
    }

    func testSettingsSavedBeforeBotLogosKeepThemOn() throws {
        let saved = try JSONDecoder().decode(Settings.self, from: Data("{}".utf8))
        XCTAssertTrue(saved.botLogos)
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
        let models = sixRows
        let interleaved = [models[0], models[3], models[1], models[4], models[2], models[5]]
        XCTAssertEqual(interleaved.agentGroups.map(\.id), ["Claude", "Codex", "Antigravity", "ChatGPT", "DeepSeek"])
        XCTAssertEqual(interleaved.groupedAgentOrder, [models[0], models[1], models[3], models[4], models[2], models[5]])
    }

    func testMovingGroupsInBothDirectionsKeepsModelsTogether() {
        let models = sixRows
        let moved = models.movingGroup(id: "Claude", to: "Antigravity")
        XCTAssertEqual(moved, [models[2], models[3], models[4], models[0], models[1], models[5]])
        XCTAssertEqual(moved.movingGroup(id: "Claude", to: "ChatGPT"), models)
    }

    func testMovingReorders() {
        let ids = sixRows.moving(id: "codex", to: 0).map(\.id)
        XCTAssertEqual(ids.first, "codex")
        XCTAssertEqual(ids.count, sixRows.count)
        XCTAssertEqual(sixRows.moving(id: "nope", to: 0), sixRows)
        XCTAssertEqual(sixRows.moving(id: "codex", to: 99), sixRows)
    }

    func testAllModelsIsSavedOnlyWhereFalse() throws {
        let scoped = AgentDescriptor(id: "claude-weekly-fable", vendor: "Claude", model: L10n.windowWeeklyPrefix + "Fable", source: "",
                                     enabled: true, allModels: false)
        let every = AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: L10n.windowWeekly, source: "", enabled: true)
        func keys(_ agent: AgentDescriptor) throws -> Set<String> {
            Set((try JSONSerialization.jsonObject(with: JSONEncoder().encode(agent)) as! [String: Any]).keys)
        }
        XCTAssertTrue(try keys(scoped).contains("allModels"))
        XCTAssertFalse(try keys(every).contains("allModels"))
        XCTAssertEqual(try JSONDecoder().decode([AgentDescriptor].self, from: JSONEncoder().encode([scoped, every])), [scoped, every])
        XCTAssertFalse(scoped.with(enabled: false).allModels)
    }

    func testWithKeepsOtherFields() {
        let a = sixRows[0].with(enabled: false)
        XCTAssertFalse(a.enabled)
        XCTAssertEqual(a.source, sixRows[0].source)
        XCTAssertEqual(a.displayName, "Claude · Opus 5.5")
    }

    /// Every surface names a row through its descriptor: a window key in words, a model as written.
    func testNamesComeFromTheDescriptor() {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        let session = AgentDescriptor(id: "claude-session", vendor: "Claude", model: L10n.windowSession, source: "", enabled: true)
        XCTAssertEqual([session.name, session.shortName, session.displayName, session.compactName],
                       ["Current session", "5h", "Claude · Current session", "Claude · 5h"])
        let spark = AgentDescriptor(id: "codex:spark:primary", vendor: "Codex", model: "GPT-5.3-Codex-Spark · 5h limit", shortModel: "Spark 5h",
                                    source: "", enabled: true)
        XCTAssertEqual([spark.name, spark.shortName, spark.compactName], ["GPT-5.3-Codex-Spark · 5h limit", "Spark 5h", "Codex · Spark 5h"],
                       "a window's short name is its provider's")
        let model = sixRows[0]
        XCTAssertEqual([model.name, model.shortName, model.compactName], ["Opus 5.5", "Opus", "Claude · Opus"])
    }
}

final class AgentSettingsTests: XCTestCase {
    func testAccountDetailsUseObservedPlansAndAPIProvidersWithoutGuessing() {
        let now = Date()
        let sources = [SourceStatus(id: "cursor", name: "Cursor", detail: "technical details", state: .installed),
                       SourceStatus(id: "deepseek", name: "DeepSeek", detail: "", state: .installed)]
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

    func testGroupsIncludeUndetectedSourcesWithoutWindowsAndPreserveWindowOrder() {
        let sources = [
            SourceStatus(id: "claude-code", name: "Claude", detail: "", state: .ready(plan: "max_20x")),
            SourceStatus(id: "cursor", name: "Cursor", detail: "", state: .notDetected),
            SourceStatus(id: "codex-cli", name: "Codex", detail: "", state: .installed),
            SourceStatus(id: "grok", name: "Grok", detail: "", state: .installed),
        ]
        let agents = [
            AgentDescriptor(id: "cx", vendor: "Codex", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "c1", vendor: "Claude", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "c2", vendor: "Claude", model: "Weekly", source: "", enabled: false),
        ]
        let groups = AgentSettingsGroup.make(sources: sources, agents: agents)
        XCTAssertEqual(groups.map(\.id), ["Codex", "Claude", "Grok", "Cursor"], "undetected sources follow detected groups without changing their relative order")
        XCTAssertEqual(groups[1].agents.map(\.id), ["c1", "c2"])
        XCTAssertEqual(groups[1].displayedCount(settings: Settings()), 1)
        XCTAssertEqual(groups[1].agents.count, 2)
        XCTAssertEqual(groups[2].displayedCount(settings: Settings()), 0)
        XCTAssertTrue(groups[2].agents.isEmpty)
        XCTAssertEqual(groups[3].source?.state, .notDetected)
        XCTAssertTrue(groups[3].agents.isEmpty)
        let hidden = AgentSettingsGroup.make(sources: sources, agents: agents.map { $0.with(enabled: false) })
        XCTAssertEqual(hidden.map { $0.displayedCount(settings: Settings()) }, [0, 0, 0, 0])
        XCTAssertEqual(hidden[1].agents.count, 2)
    }

    func testUndetectedGroupsMoveLastWhileEachSectionAndItsWindowsKeepTheirOrder() {
        let sources = [
            SourceStatus(id: "grok", name: "Grok", detail: "", state: .notDetected),
            SourceStatus(id: "claude-code", name: "Claude", detail: "", state: .ready(plan: "max_20x")),
            SourceStatus(id: "cursor", name: "Cursor", detail: "", state: .notDetected),
            SourceStatus(id: "codex-cli", name: "Codex", detail: "", state: .installed),
        ]
        let agents = [
            AgentDescriptor(id: "cursor-weekly", vendor: "Cursor", model: "Weekly", source: "", enabled: false),
            AgentDescriptor(id: "legacy", vendor: "Legacy", model: "Plan", source: "", enabled: true),
            AgentDescriptor(id: "claude", vendor: "Claude", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "cursor-daily", vendor: "Cursor", model: "Daily", source: "", enabled: true),
        ]
        let report = UsageReport(generatedAt: Date(), snapshots: [], sessions: [], services: [
            .init(client: "Service extra", provider: "OpenAI", product: .api),
            .init(client: "Cursor", provider: "Anthropic", product: .api),
        ])
        let groups = AgentSettingsGroup.make(sources: sources, agents: agents, report: report)
        XCTAssertEqual(groups.map(\.id), ["Legacy", "Claude", "Codex", "Service extra", "Cursor", "Grok"])
        XCTAssertNil(groups[0].source, "existing rows without a detected-source entry stay in the first section")
        XCTAssertNil(groups[3].source, "reported services without a detected-source entry stay in the first section")
        XCTAssertEqual(groups[3].apiProviders, ["OpenAI"])
        XCTAssertEqual(groups[4].agents, [agents[0], agents[3]], "partitioning groups preserves each window and its switch")
        XCTAssertEqual(groups[4].apiProviders, ["Anthropic"], "an undetected group retains reported service details")
        XCTAssertEqual(groups[4].displayedCount(settings: Settings()), 1)
        XCTAssertTrue(groups[5].agents.isEmpty)
    }

    func testEverySupportedSourceRemainsListedWhenNoneIsInstalledWithoutInventingReadings() {
        // The detector uses these existing catalogs plus the core clients; no second table of supported names.
        let enumerated = Set(AdditionalSource.allCases.map(\.vendor) + OpenAgentSource.allCases.map(\.name) + ["Grok CLI", "Grok Bot"])
        let coreClients = SessionSource.agentVendors.filter { !enumerated.contains($0) }
        let sources = coreClients.map {
            SourceStatus(id: $0.lowercased(), name: $0, detail: "", state: .notDetected)
        } + AdditionalSource.allCases.flatMap { source -> [SourceStatus] in
            guard source == .grok else {
                return [SourceStatus(id: source.rawValue, name: source.vendor, detail: source.detail, state: .notDetected)]
            }
            return [SourceStatus(id: "grok", name: "Grok CLI", detail: "", state: .notDetected, provider: "Grok"),
                    SourceStatus(id: "grok-bot", name: "Grok Bot", detail: "", state: .notDetected, provider: "Grok", supportsLiveStatus: false)]
        } + OpenAgentSource.allCases.map {
            SourceStatus(id: $0.rawValue, name: $0.name, detail: $0.detail, state: .notDetected)
        }
        XCTAssertEqual(sources.count, 18)
        XCTAssertEqual(Set(sources.map(\.name)).count, sources.count)
        let groups = AgentSettingsGroup.make(sources: sources, agents: [])
        var seen = Set<String>()
        let providers = sources.filter { seen.insert($0.provider).inserted }
        XCTAssertEqual(groups.map(\.id), providers.map(\.provider), "Execution clients share their account provider's settings group")
        XCTAssertEqual(groups.compactMap(\.source), providers)
        XCTAssertTrue(groups.allSatisfy { $0.agents.isEmpty && $0.accounts.isEmpty && $0.billingAccounts.isEmpty
            && $0.unobservedAccounts.isEmpty && $0.plans.isEmpty && $0.apiProviders.isEmpty
            && $0.displayedCount(settings: Settings()) == 0 }, "An undetected source is not a quota window or an observed account")
    }

    func testGroupsListOnlyRowsAProviderStillReports() {
        let agents = [
            AgentDescriptor(id: "cx", vendor: "Codex", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "gone", vendor: "Codex", model: "Weekly", source: "", enabled: true),
            AgentDescriptor(id: "retired-vendor", vendor: "Retired", model: "Plan", source: "", enabled: true),
        ]
        let report = UsageReport(generatedAt: Date(), snapshots: [], sessions: [], discoveredAgents: [agents[0]],
                                 rowSeenAt: ["cx": Date()])
        let groups = AgentSettingsGroup.make(sources: [], agents: agents, report: report)
        XCTAssertEqual(groups.map(\.id), ["Codex"])
        XCTAssertEqual(groups[0].agents.map(\.id), ["cx"])
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
        let store = SettingsStore(defaults: defaults, defaultAgents: sixRows)
        store.update {
            $0.glowRange = 18
            $0.glowOutwardOnly = false
            $0.openImmediatelyAtTop = true
        }
        store.setAgent(id: "chatgpt", enabled: false)
        store.moveAgent(id: "codex", to: 0)
        store.markOnboardingComplete()

        let reloaded = SettingsStore(defaults: defaults)
        XCTAssertEqual(reloaded.settings.glowRange, 18)
        XCTAssertFalse(reloaded.settings.glowOutwardOnly)
        XCTAssertTrue(reloaded.settings.openImmediatelyAtTop)
        XCTAssertEqual(reloaded.agents.first?.id, "codex")
        XCTAssertFalse(reloaded.agents.first { $0.id == "chatgpt" }!.enabled)
        XCTAssertTrue(reloaded.hasCompletedOnboarding)
        XCTAssertEqual(reloaded.enabledAgents.count, 3)
    }

    func testFreshStoreUsesDefaults() {
        let store = SettingsStore(defaults: makeDefaults())
        XCTAssertEqual(store.settings, Settings())
        XCTAssertTrue(store.agents.isEmpty, "every row is discovered from local data")
        XCTAssertFalse(store.hasCompletedOnboarding)
    }

    func testPlaceholdersEarlierVersionsSeededAreDroppedAndOtherRowsKeepTheirSwitches() throws {
        let defaults = makeDefaults()
        let stored = [
            AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: "Weekly", source: "", enabled: false),
            AgentDescriptor(id: "chatgpt", vendor: "ChatGPT", model: "ChatGPT", source: L10n.sourceBrowserAuth, enabled: true, connected: false),
            AgentDescriptor(id: "codex", vendor: "Codex", model: "Desktop / CLI", source: L10n.sourceCodexAppServer, enabled: true, connected: false),
            AgentDescriptor(id: "cursor", vendor: "Cursor", model: "Plan usage", source: "", enabled: true),
        ]
        defaults.set(try JSONEncoder().encode(stored), forKey: SettingsStore.Keys.agents)
        let store = SettingsStore(defaults: defaults)
        XCTAssertEqual(store.agents.map(\.id), ["claude-weekly", "cursor"])
        XCTAssertFalse(store.agents[0].enabled)
    }

    func testVendorsNewToTheListArriveOnTopInProviderOrderAndDeepSeekArrivesHidden() {
        let cursor = AgentDescriptor(id: "cursor", vendor: "Cursor", model: "Plan usage", source: "", enabled: false)
        let store = SettingsStore(defaults: makeDefaults(), defaultAgents: [cursor])
        store.mergeDiscovered([
            AgentDescriptor(id: "claude-session", vendor: "Claude", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: "Weekly", source: "", enabled: true),
            AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "deepseek-model:flash", vendor: "DeepSeek", model: "flash", source: "", enabled: true),
            AgentDescriptor(id: "cursor", vendor: "Cursor", model: "Plan usage", source: "", enabled: true),
            AgentDescriptor(id: "cursor:models", vendor: "Cursor", model: "Cursor models", source: "", enabled: true),
            AgentDescriptor(id: "newcomer", vendor: "Newcomer", model: "Plan", source: "", enabled: true),
        ])
        XCTAssertEqual(store.agents.map(\.id), ["claude-session", "claude-weekly", "codex", "deepseek-model:flash", "newcomer", "cursor", "cursor:models"])
        XCTAssertEqual(store.agents.map(\.enabled), [true, true, true, false, true, false, true],
                       "the user's switch survives, DeepSeek starts hidden, and a vendor the catalog does not list arrives on")
    }

    func testGroupedOrderPersistsWithModelToggles() throws {
        let defaults = makeDefaults()
        let models = sixRows
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

    func testMergeDiscoveredRefreshesARowsNamesAndWhetherItCoversEveryModel() throws {
        let defaults = makeDefaults()
        // Saved before the flag existed, with its switch off.
        defaults.set(Data(#"[{"id":"claude-weekly-fable","vendor":"Claude","model":"window.weekly.Fable","source":"","enabled":false,"connected":true}]"#.utf8),
                     forKey: SettingsStore.Keys.agents)
        let store = SettingsStore(defaults: defaults)
        XCTAssertEqual(store.agents.map(\.allModels), [true], "a row saved without the flag reads as covering every model")
        store.mergeDiscovered([AgentDescriptor(id: "claude-weekly-fable", vendor: "Claude", model: "window.weekly.Fable", source: "",
                                               enabled: true, allModels: false)])
        XCTAssertEqual(store.agents.map(\.allModels), [false])
        XCTAssertEqual(store.agents.map(\.enabled), [false], "the switch survives")
        XCTAssertEqual(SettingsStore(defaults: defaults).agents.map(\.allModels), [false], "the flag is saved")
        // A window saved under an earlier name takes its provider's names.
        store.mergeDiscovered([AgentDescriptor(id: "codex", vendor: "Codex", model: "Weekly", source: "", enabled: true)])
        store.mergeDiscovered([AgentDescriptor(id: "codex", vendor: "Codex", model: "Weekly limit", shortModel: "Weekly", source: "", enabled: true)])
        let codex = try XCTUnwrap(SettingsStore(defaults: defaults).agents.first { $0.id == "codex" })
        XCTAssertEqual([codex.name, codex.shortName], ["Weekly limit", "Weekly"])
    }

    func testMergeDiscoveredInsertsByVendorAndUpdatesNames() {
        let store = SettingsStore(defaults: makeDefaults())
        let fable = AgentDescriptor(id: "claude-fable", vendor: "Claude", model: "Fable 5.1", source: "Claude Code", enabled: true)
        let sonnet = AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 5", source: "Claude Code", enabled: true)
        store.mergeDiscovered([fable, sonnet])
        XCTAssertEqual(store.agents.map(\.id), ["claude-fable", "claude-sonnet"], "new vendor goes to the top, in discovery order")

        store.setAgent(id: "claude-sonnet", enabled: false)
        store.moveAgent(id: "claude-sonnet", to: 0)
        let haiku = AgentDescriptor(id: "claude-haiku", vendor: "Claude", model: "Haiku 4.5", source: "Claude Code", enabled: false)
        store.mergeDiscovered([AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 5.1", source: "Claude Code", enabled: true), haiku])
        XCTAssertEqual(store.agents.map(\.id), ["claude-sonnet", "claude-fable", "claude-haiku"], "existing order kept, new family after the last Claude row")
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

    private func makeStore(rows: [AgentDescriptor] = sixRows) -> UsageStore {
        let suite = "AgentHUDTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        return UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: rows))
    }

    func testRowsNoProviderReportsAreHiddenButKeepTheirSwitch() {
        let store = makeStore()
        let all = store.settings.agents
        func report(listing rows: [AgentDescriptor]) -> UsageReport {
            UsageReport(generatedAt: Date(), snapshots: [], sessions: [], discoveredAgents: rows,
                        rowSeenAt: Dictionary(uniqueKeysWithValues: rows.map { ($0.id, Date()) }))
        }
        store.replace(report: report(listing: all.filter { $0.id != "chatgpt" }))
        XCTAssertFalse(store.visibleAgents.contains { $0.id == "chatgpt" })
        XCTAssertEqual(store.rows.map(\.id), ["claude-opus", "claude-sonnet", "codex"])
        XCTAssertTrue(store.settings.agents.contains { $0.id == "chatgpt" && $0.enabled }, "its switch waits for the row to come back")
        store.replace(report: report(listing: all))
        XCTAssertEqual(store.rows.map(\.id), ["claude-opus", "claude-sonnet", "chatgpt", "codex"])
    }

    func testTheDemoKeepsItsRowsWhenItsReportsAreKept() async {
        let store = UsageStore(provider: RetainedUsageProvider(provider: DemoUsageProvider()),
                               settings: SettingsStore(defaults: UserDefaults(suiteName: "AgentHUDTests.\(UUID().uuidString)")!,
                                                       defaultAgents: DemoData.agents))
        await store.refresh()
        XCTAssertEqual(store.rows.map(\.id), (DemoData.claudeRows + DemoData.codexRows).map(\.id))
    }

    func testAgentsWithoutDataStayOutOfGlow() async {
        let suite = "AgentHUDTests.\(UUID().uuidString)"
        let codex = DemoData.codexRows[0]
        let rows = [codex, AgentDescriptor(id: "newcomer", vendor: "Newcomer", model: "Plan", source: "", enabled: true)]
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: UserDefaults(suiteName: suite)!, defaultAgents: rows))
        XCTAssertEqual(store.levels, [])
        XCTAssertTrue(store.isLoading)
        await store.refresh()
        XCTAssertFalse(store.isLoading)
        XCTAssertEqual(store.rows.map(\.id), [codex.id, "newcomer"])
        XCTAssertEqual(store.levels.count, 1, "only the row demo data has a reading for glows")
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
        let store = makeStore(rows: DemoData.agents)
        await store.refresh()
        XCTAssertEqual(store.rows.map(\.id), (DemoData.claudeRows + DemoData.codexRows).map(\.id))
        XCTAssertEqual(store.rows.map(\.level), [.ok, .warning, .ok, .ok, .ok, .ok], "Claude's week is 79% used")
        XCTAssertEqual(store.maxUsedPct, 79)
        XCTAssertTrue(store.hasLiveSession)

        let filling = DemoStage(script: DemoScript(used: ["codex": 93], steps: []), now: Date())
        store.replace(report: DemoUsageProvider.report(agents: store.settings.agents, historyHours: 24, now: Date(), stage: filling))
        XCTAssertEqual(store.rows.map(\.level), [.ok, .warning, .ok, .critical, .ok, .ok])
        XCTAssertEqual(store.maxUsedPct, 93)
    }

    func testAPIBillingReplacesQuotaRowAndFollowsAgentToggle() throws {
        let store = makeStore()
        let deepseek = AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceDeepSeekSessions, enabled: true)
        let claude = sixRows[0]
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

    func testTokenChartsIncludeAllModelsRegardlessOfQuotaSwitches() {
        let store = makeStore()
        let hour = Calendar.current.dateInterval(of: .hour, for: Date())!.start
        let usage = [("claude-opus", 100), ("codex", 200), ("antigravity", 900)].map { id, tokens in
            UsageBucket(start: hour, agentId: id, tokensIn: tokens, tokensOut: 0)
        }
        store.replace(report: UsageReport(generatedAt: Date(), snapshots: [], sessions: [],
            consumers: sixRows, usage: usage))

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
        store.settings.panelLayout.setSwitches { $0.resetCountdown = false }
        XCTAssertEqual(try XCTUnwrap(store.quotaForecastHint(for: "codex")), "耗尽 ~2小时30分",
                      "hiding the reset column does not hide the hover forecast")
    }

    func testQuotaTokenRateUsesOnlyMappedConsumersInTheObservedCurrentCycle() throws {
        let weekly = AgentDescriptor(id: "claude-weekly", vendor: "Claude", model: "Weekly", source: "", enabled: true)
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: UserDefaults(suiteName: "AgentHUDTests.\(UUID().uuidString)")!,
                                                                                      defaultAgents: sixRows + [weekly]))
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
        let store = makeStore(rows: DemoData.agents)
        await store.refresh()
        XCTAssertEqual(store.levels, [.ok, .warning, .ok, .ok, .ok, .ok, .warning], "six windows, then DeepSeek's balance")
        store.settings.setAgent(id: DemoData.claudeRows[1].id, enabled: false)
        XCTAssertEqual(store.levels, [.ok, .ok, .ok, .ok, .ok, .warning])
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

    func testSessionListCoversTheLastWeekWhateverTheChartRange() {
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
        // A session read in flight is listed whenever it started, with its reading as its last event.
        for range in StatsRange.allCases {
            store.setStatsRange(range)
            XCTAssertEqual(store.statsSessions.map(\.id), ["future", "running", "recent", "today", "week"], "\(range)")
        }
    }
}
