import XCTest
@testable import AgentHUDCore

final class ProviderAccountTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private let accountA = ProviderAccount.identified(provider: "Codex", user: "a@example.com", workspace: "workspace-1")!
    private let accountB = ProviderAccount.identified(provider: "Codex", user: "b@example.com", workspace: "workspace-1")!

    func testAccountIdsIgnoreEvidenceAndSeparateUsersWorkspacesAndHomes() throws {
        let confirmed = try XCTUnwrap(ProviderAccount.identified(provider: "Grok", user: "u", workspace: "t"))
        let local = try XCTUnwrap(ProviderAccount.identified(provider: "Grok", user: "u", workspace: "t", evidence: .credential))
        XCTAssertEqual(confirmed.id, local.id, "confirming an identity later keeps the storage key")
        XCTAssertNotEqual(accountA.id, accountB.id, "members of one workspace have separate quota")
        XCTAssertNotEqual(accountA.id, ProviderAccount.identified(provider: "Codex", user: "a@example.com", workspace: "workspace-2")!.id)
        XCTAssertNotEqual(ProviderAccount.unresolved(provider: "Codex", home: "").id, ProviderAccount.unresolved(provider: "Codex", home: "work").id)
        XCTAssertNil(ProviderAccount.identified(provider: "Codex", user: " ", workspace: nil))
        let row = AgentDescriptor(id: accountA.windowID("codex:spark:primary"), vendor: "Codex", model: "", source: "", enabled: true, account: accountA)
        XCTAssertEqual(row.windowKey, "codex:spark:primary")
        let pool = BillingPool(provider: "Kimi", realm: "CN", product: .plan, scope: "s", evidence: .account, entitlement: "kimi-code")
        let poolRow = AgentDescriptor(id: pool.windowID("weekly"), vendor: "Kimi", model: "", source: "", enabled: true, billingPool: pool,
                                      account: ProviderAccount(pool: pool))
        XCTAssertEqual(poolRow.windowKey, "weekly")
        XCTAssertEqual(poolRow.account?.id, pool.id, "pool rows keep their existing ids")
    }

    func testCodexRowsBelongToTheSignedInWorkspaceMember() throws {
        let json = #"{"accountId":"workspace-1","rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":10,"windowDurationMins":300}}},"account":{"type":"chatgpt","email":"A@Example.com","planType":"team"}}"#
        let limits = try JSONDecoder().decode(CodexRateLimits.self, from: Data(json.utf8))
        XCTAssertEqual(limits.providerAccount(home: ""), accountA, "email case does not split an account")
        XCTAssertEqual(limits.rows(home: "").map(\.id), [accountA.windowID("codex")])
        XCTAssertEqual(limits.rows(home: "").first?.descriptor.account, accountA)
        XCTAssertEqual(limits.plan, "team", "account/read supplies the plan when buckets do not")
        let anonymous = try JSONDecoder().decode(CodexRateLimits.self, from: Data(#"{"rateLimits":{"primary":{"usedPercent":5}}}"#.utf8))
        XCTAssertEqual(anonymous.providerAccount(home: "work").evidence, .unresolved)
    }

    func testSwitchingAccountsKeepsTheLastReadingWithoutMixingCredits() async throws {
        let provider = RetainedUsageProvider(provider: Sequence([
            report(account: accountA, remaining: 0, at: now, credits: 2),
            report(account: accountB, remaining: 100, at: now.addingTimeInterval(120), credits: nil)
        ]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let switched = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(switched.snapshot(for: accountA.windowID("codex"))?.remainingPct, 0, "the previous account keeps its last reading")
        XCTAssertEqual(switched.snapshot(for: accountA.windowID("codex"))?.updatedAt, now)
        XCTAssertEqual(switched.snapshot(for: accountB.windowID("codex"))?.remainingPct, 100)
        XCTAssertEqual(switched.accounts?["Codex"]?.map(\.isCurrent), [true, false])
        XCTAssertFalse(switched.isCurrent(descriptor(accountA)))
        XCTAssertTrue(switched.isCurrent(descriptor(accountB)))
        XCTAssertNil(switched.codexResetCredits, "earned resets belong to the account that reported them")
    }

    func testACodexReadingWithoutItsEmailKeepsItsAccountAcrossLaunches() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-codex-identity-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let json = #"{"accountId":"workspace-1","rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":10,"windowDurationMins":300}}},"account":{"type":"chatgpt","email":"A@Example.com","planType":"prolite"}}"#
        let signedIn = try JSONDecoder().decode(CodexRateLimits.self, from: Data(json.utf8))
        var late = signedIn
        late.account = nil  // account/read answered after the grace
        final class Readings: @unchecked Sendable {
            var queue: [CodexRateLimits]
            var now: Date
            init(_ queue: [CodexRateLimits], now: Date) { self.queue = queue; self.now = now }
        }
        let cache = directory.appendingPathComponent("codex-identities.json")
        func provider(_ readings: Readings) -> CodexUsageProvider {
            CodexUsageProvider(readLimits: { readings.queue.removeFirst() }, transcripts: CodexTranscriptStore(roots: [directory]),
                               history: QuotaHistoryStore(), clock: { readings.now }, identityCacheURL: cache)
        }
        let readings = Readings([signedIn, late], now: now)
        let running = provider(readings)
        let first = try await running.fetchAccountAndLocalUsage(agents: [], historyHours: 1)
        XCTAssertEqual(first.accounts?["Codex"]?.map(\.account), [accountA])
        XCTAssertEqual(first.accounts?["Codex"]?.first?.aliases, [ProviderAccount.identified(provider: "Codex", user: nil, workspace: "workspace-1")!.id])
        readings.now = now.addingTimeInterval(UsageRefresh.accountRequestSpacing)
        let second = try await running.fetchAccountAndLocalUsage(agents: [], historyHours: 1)
        XCTAssertEqual(second.accounts?["Codex"]?.map(\.account), [accountA], "a late account/read does not file the account again")
        XCTAssertEqual(second.accounts?["Codex"]?.first?.label, "A@Example.com")
        let relaunched = try await provider(Readings([late], now: now)).fetchAccountAndLocalUsage(agents: [], historyHours: 1)
        XCTAssertEqual(relaunched.accounts?["Codex"]?.map(\.account), [accountA], "the first reading after a launch keeps it too")
    }

    func testAnAccountReadWithItsEmailRetiresTheKeyItHadWithoutIt() async throws {
        let withoutEmail = ProviderAccount.identified(provider: "Codex", user: nil, workspace: "workspace-1")!
        let later = now.addingTimeInterval(120)
        let complete = UsageReport(generatedAt: later, snapshots: [.init(agentId: accountA.windowID("codex"), remainingPct: 59, updatedAt: later)],
                                   sessions: [], discoveredAgents: [descriptor(accountA)],
                                   accounts: ["Codex": [AccountObservation(account: accountA, label: "a@example.com", observedAt: later,
                                                                           aliases: [withoutEmail.id])]])
        let provider = RetainedUsageProvider(provider: Sequence([report(account: withoutEmail, remaining: 60, at: now, credits: nil), complete]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let merged = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(merged.accounts?["Codex"]?.map(\.account), [accountA], "the key read without the email was the same account")
        XCTAssertNil(merged.snapshot(for: withoutEmail.windowID("codex")), "its last reading leaves with it")
    }

    @MainActor
    func testSettingsAndQuotaSectionsShareOneSummaryPerAccountAcrossClientHomes() throws {
        let otherWorkspace = ProviderAccount.identified(provider: "Codex", user: "a@example.com", workspace: "workspace-2")!
        let old = AccountObservation(account: accountA, label: "a@example.com", plan: "plus", observedAt: now, isCurrent: false)
        let current = AccountObservation(account: accountA, home: "pi:", label: "a@example.com", plan: "plus",
                                         observedAt: now.addingTimeInterval(60))
        let agents = [accountA, accountB, otherWorkspace].map(descriptor)
        let report = UsageReport(generatedAt: now.addingTimeInterval(60), snapshots: agents.map {
            .init(agentId: $0.id, remainingPct: 100, updatedAt: now)
        }, sessions: [], discoveredAgents: agents, accounts: ["Codex": [
            old, current,
            .init(account: accountB, label: "b@example.com", plan: "pro", observedAt: now),
            .init(account: otherWorkspace, label: "a@example.com", plan: "plus", observedAt: now, isCurrent: false)
        ]])
        let group = try XCTUnwrap(AgentSettingsGroup.make(sources: [], agents: agents, report: report).first)
        XCTAssertEqual(group.accounts.count, 3, "client-home history is not another account")
        XCTAssertEqual(group.accounts.filter { $0.account == accountA }, [current])
        XCTAssertTrue(group.accounts.contains { $0.account == otherWorkspace }, "same email in another workspace stays separate")
        XCTAssertEqual(report.accounts?["Codex"]?.count, 4, "display grouping preserves source observations")

        let suite = "AccountSummaryTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: agents)
        let store = UsageStore(provider: DemoUsageProvider(), settings: settings)
        store.replace(report: report)
        let panelAccounts = store.accountSections(store.rows).compactMap(\.account)
        XCTAssertEqual(Set(group.accounts), Set(panelAccounts), "settings and quota surfaces use the same account summaries")
    }

    @MainActor
    func testSuccessfulCodexInventoryRetiresMissingWindowsButKeepsOtherAccounts() async throws {
        let spark = AgentDescriptor(id: accountA.windowID("codex:spark:primary"), vendor: "Codex", model: "Spark", source: "", enabled: true, account: accountA)
        let original = UsageReport(generatedAt: now, snapshots: [
            .init(agentId: spark.id, remainingPct: 100, updatedAt: now),
            .init(agentId: accountB.windowID("codex"), remainingPct: 0, updatedAt: now)
        ], sessions: [], discoveredAgents: [spark, descriptor(accountB)], accounts: ["Codex": [
            .init(account: accountA, observedAt: now), .init(account: accountB, observedAt: now, isCurrent: false)
        ]])
        let provider = RetainedUsageProvider(provider: Sequence([original, report(account: accountA, remaining: 100, at: now.addingTimeInterval(60), credits: nil)]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let updated = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(updated.snapshot(for: spark.id))
        XCTAssertFalse(updated.discoveredAgents.contains { $0.id == spark.id })
        XCTAssertNotNil(updated.snapshot(for: accountB.windowID("codex")))
        let suite = "InventoryTests.\(UUID().uuidString)", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [spark, descriptor(accountB)])
        settings.mergeDiscovered(updated.discoveredAgents, accounts: updated.accounts, replaceQuotaWindows: true)
        XCTAssertFalse(settings.agents.contains { $0.id == spark.id })
        XCTAssertTrue(settings.agents.contains { $0.account == accountB })
    }

    func testFailedPollKeepsTheCurrentAccountAndUnseenAccountsRetire() async throws {
        let old = report(account: accountA, remaining: 40, at: now, credits: 1)
        let failed = UsageReport(generatedAt: now.addingTimeInterval(60), snapshots: [], sessions: [], sourceNotices: ["Codex": "offline"])
        let provider = RetainedUsageProvider(provider: Sequence([old, failed, report(account: accountB, remaining: 90, at: now.addingTimeInterval(31 * 86400), credits: nil)]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let offline = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertTrue(offline.isCurrent(descriptor(accountA)))
        XCTAssertEqual(offline.codexResetCredits?.availableCount, 1)
        let later = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(later.accounts?["Codex"]?.map(\.account), [accountB])
        XCTAssertNil(later.snapshot(for: accountA.windowID("codex")), "an account unseen for the retention period retires")
        XCTAssertFalse(later.discoveredAgents.contains { $0.account == accountA })
    }

    @MainActor
    func testForgottenProviderRetiresReadingsAndSettingsAtOnce() async throws {
        let signedOut = UsageReport(generatedAt: now.addingTimeInterval(60), snapshots: [], sessions: [], accounts: ["Codex": []])
        let forgotten = UsageReport(generatedAt: now.addingTimeInterval(120), snapshots: [], sessions: [], forgottenAccountProviders: ["Codex"])
        let provider = RetainedUsageProvider(provider: Sequence([report(account: accountA, remaining: 40, at: now, credits: nil), signedOut, forgotten]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let kept = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNotNil(kept.snapshot(for: accountA.windowID("codex")), "no current account still shows the last reading")
        let cleared = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(cleared.snapshot(for: accountA.windowID("codex")))
        XCTAssertEqual(cleared.accounts?["Codex"], [])
        let suite = "ProviderAccountTests.\(UUID().uuidString)", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [descriptor(accountA)])
        settings.mergeDiscovered(cleared.discoveredAgents, accounts: cleared.accounts)
        XCTAssertTrue(settings.agents.isEmpty)
    }

    func testUnscopedReadingsAreDroppedOnceTheProviderIdentifiesAccounts() async throws {
        let legacyRow = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: "", enabled: true)
        let legacy = UsageReport(generatedAt: now, snapshots: [.init(agentId: "codex", remainingPct: 12, updatedAt: now)], sessions: [],
                                 discoveredAgents: [legacyRow])
        let provider = RetainedUsageProvider(provider: Sequence([legacy, report(account: accountA, remaining: 80, at: now.addingTimeInterval(60), credits: nil)]))
        _ = try await provider.fetchUsage(agents: [], historyHours: 24)
        let migrated = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(migrated.snapshot(for: "codex"))
        XCTAssertEqual(migrated.discoveredAgents.map(\.id), [accountA.windowID("codex")])
    }

    @MainActor
    func testSettingsMoveToTheFirstAccountAndLaterAccountsInheritSwitches() {
        let suite = "ProviderAccountTests.\(UUID().uuidString)", defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [
            AgentDescriptor(id: "claude-session", vendor: "Claude", model: "5h", source: "", enabled: true),
            AgentDescriptor(id: "codex", vendor: "Codex", model: "Desktop / CLI", source: "", enabled: false)
        ])
        let first = report(account: accountA, remaining: 50, at: now, credits: nil)
        settings.mergeDiscovered(first.discoveredAgents, accounts: first.accounts)
        XCTAssertEqual(settings.agents.map(\.id), ["claude-session", accountA.windowID("codex")])
        XCTAssertFalse(settings.agents[1].enabled, "the switch and position move with the window")
        var inventory = first.accounts!
        inventory["Codex"]!.append(AccountObservation(account: accountB, observedAt: now))
        settings.mergeDiscovered([descriptor(accountB)], accounts: inventory)
        XCTAssertEqual(settings.agents.map(\.id), ["claude-session", accountA.windowID("codex"), accountB.windowID("codex")])
        XCTAssertFalse(settings.agents[2].enabled)
        settings.mergeDiscovered([], accounts: ["Codex": [AccountObservation(account: accountB, observedAt: now)]])
        XCTAssertEqual(settings.agents.map(\.id), ["claude-session", accountB.windowID("codex")], "retired accounts leave the settings")
    }

    func testSwitchingAccountsIsNeitherAResetNorAnExhaustion() {
        var tracker = QuotaAlertTracker()
        let agents = [descriptor(accountA), descriptor(accountB)]
        func update(_ current: ProviderAccount, remaining: Double, elapsed: Double) -> QuotaAlertTracker.Update {
            let other = current == accountA ? accountB : accountA
            let at = now.addingTimeInterval(elapsed)
            let snapshot = UsageSnapshot(agentId: current.windowID("codex"), remainingPct: remaining, resetAt: at.addingTimeInterval(3600), updatedAt: at)
            let report = UsageReport(generatedAt: at, snapshots: [snapshot], sessions: [],
                accounts: ["Codex": [AccountObservation(account: current, observedAt: at), AccountObservation(account: other, observedAt: now, isCurrent: false)]])
            return tracker.update(report: report, agents: agents, now: at)
        }
        _ = update(accountA, remaining: 0, elapsed: 0)
        XCTAssertTrue(update(accountB, remaining: 100, elapsed: 120).alerts.isEmpty)
        XCTAssertTrue(update(accountB, remaining: 100, elapsed: 240).alerts.isEmpty)
        let back = update(accountA, remaining: 100, elapsed: 360)
        XCTAssertTrue(back.alerts.isEmpty, "signing back in starts a new baseline")
        XCTAssertTrue(back.criticalAgentIDs.isEmpty)
    }

    func testClaudeLoginChangeDuringTheQueryDiscardsTheReading() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-account-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let profile = directory.appendingPathComponent(".claude.json")
        try #"{"oauthAccount":{"accountUuid":"one","organizationUuid":"org"}}"#.write(to: profile, atomically: true, encoding: .utf8)
        let response = #"{"type":"control_response","response":{"subtype":"success","response":{"subscription_type":"pro","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":10}}}}}"#
        let engine = directory.appendingPathComponent("claude")
        try "#!/bin/sh\nread -r request\nprintf '%s' '{\"oauthAccount\":{\"accountUuid\":\"two\",\"organizationUuid\":\"org\"}}' > '\(profile.path)'\necho '\(response)'\n"
            .write(to: engine, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: engine.path)
        let provider = ClaudeCodeProvider(engine: .init(executable: engine, workingDirectory: directory), transcripts: .init(roots: []),
                                          history: .init(), accountProfileURL: profile)
        do {
            _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 1)
            XCTFail("a reading taken across a login change has no owner")
        } catch let error as ClaudeDataError {
            XCTAssertEqual(error, .accountChanged)
        }
    }

    private func descriptor(_ account: ProviderAccount) -> AgentDescriptor {
        AgentDescriptor(id: account.windowID("codex"), vendor: "Codex", model: "5h", source: "", enabled: true, account: account)
    }

    private func report(account: ProviderAccount, remaining: Double, at date: Date, credits: Int?) -> UsageReport {
        UsageReport(generatedAt: date, snapshots: [.init(agentId: account.windowID("codex"), remainingPct: remaining, updatedAt: date)],
                    sessions: [], discoveredAgents: [descriptor(account)],
                    codexResetCredits: credits.map { .init(availableCount: $0, credits: nil) }, codexResetCreditsObservedAt: credits == nil ? nil : date,
                    accounts: ["Codex": [AccountObservation(account: account, observedAt: date)]])
    }
}

private actor Sequence: UsageProvider {
    var reports: [UsageReport]
    init(_ reports: [UsageReport]) { self.reports = reports }
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) throws -> UsageReport {
        guard !reports.isEmpty else { throw UsageProviderError("offline") }
        return reports.removeFirst()
    }
}
