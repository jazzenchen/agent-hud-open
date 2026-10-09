import XCTest
@testable import AgentHUDCore

final class WalletAccountObservationTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testWalletObservationsKeepTheirOwnTimesAndUnknownQuotaDoesNotEnterHistory() async throws {
        let account = ProviderAccount.identified(provider: "Grok", user: "fixture", workspace: nil)!
        let quotaTime = now.addingTimeInterval(-3600)
        var quota = ProviderQuota(windows: [
            .init(id: "grok", label: "Weekly", remaining: nil, reset: now.addingTimeInterval(3600)),
            .init(id: "grok:extra", label: "Extra", remaining: 100, observedAt: now)
        ], account: account, observedAt: quotaTime, client: "Grok Bot")
        quota.wallets = [AccountWallet(kind: .prepaid, balance: 0, observedAt: now),
                         AccountWallet(kind: .onDemand, used: 0, limit: 10, observedAt: now)]
        quota.sourceInfo = "Bot source"
        let reading = quota, clock = now
        let history = QuotaHistoryStore()
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { reading }, readSessions: { _ in .init() },
                                               history: history, clock: { clock })
        await provider.refreshAccountUsage(historyHours: 24)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        let observation = try XCTUnwrap(report.observation(accountID: account.id))
        XCTAssertEqual(observation.wallets, quota.wallets)
        XCTAssertEqual(observation.sourceInfo, quota.sourceInfo)
        XCTAssertEqual(observation.observedAt, quotaTime)
        XCTAssertNil(report.snapshot(for: account.windowID("grok"))?.remainingPct)
        XCTAssertEqual(report.snapshot(for: account.windowID("grok"))?.updatedAt, quotaTime)
        XCTAssertEqual(report.snapshot(for: account.windowID("grok:extra"))?.updatedAt, now)
        let unknownSamples = await history.samples(agentId: account.windowID("grok"), since: .distantPast)
        let extraSamples = await history.samples(agentId: account.windowID("grok:extra"), since: .distantPast)
        XCTAssertTrue(unknownSamples.isEmpty)
        XCTAssertEqual(extraSamples.map(\.timestamp), [now])
        XCTAssertEqual(extraSamples.map(\.remainingPct), [100])
    }

    func testWalletAndProvenanceSurviveReportCacheAndHistoricalAccountTransition() throws {
        let account = ProviderAccount.identified(provider: "Grok", user: "fixture", workspace: nil)!
        let observation = AccountObservation(account: account, client: "Grok Bot", observedAt: now,
            wallets: [AccountWallet(kind: .prepaid, balance: 0, observedAt: now.addingTimeInterval(-60)),
                      AccountWallet(kind: .onDemand, used: 0, limit: nil, observedAt: now)], sourceInfo: "Bot source")
        let previous = UsageReport(generatedAt: now, snapshots: [], sessions: [], accounts: ["Grok": [observation]])
        XCTAssertEqual(try JSONDecoder().decode(UsageReport.self, from: JSONEncoder().encode(previous.restartCopy)), previous)
        let signedOut = UsageReport(generatedAt: now.addingTimeInterval(60), snapshots: [], sessions: [], accounts: ["Grok": []])
            .retainingReadings(from: previous)
        let historical = try XCTUnwrap(signedOut.observation(accountID: account.id))
        XCTAssertFalse(historical.isCurrent)
        XCTAssertEqual(historical.wallets, observation.wallets)
        XCTAssertEqual(historical.sourceInfo, observation.sourceInfo)
        XCTAssertEqual(historical.observedAt, now)
    }

    func testOlderAccountCacheRemainsReadableWithoutWalletsOrSourceInfo() throws {
        let account = ProviderAccount.identified(provider: "Grok", user: "fixture", workspace: nil)!
        let observation = AccountObservation(account: account, observedAt: now)
        var json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(observation)) as? [String: Any])
        json.removeValue(forKey: "wallets")
        json.removeValue(forKey: "sourceInfo")
        let restored = try JSONDecoder().decode(AccountObservation.self, from: JSONSerialization.data(withJSONObject: json))
        XCTAssertNil(restored.wallets)
        XCTAssertNil(restored.sourceInfo)
        XCTAssertEqual(restored.account, account)
    }

    func testIncompleteAndZeroSpendingCapsReplaceThePreviousExtraPercentage() async throws {
        let known = try GrokClient.parse(.read(Data(#"{"config":{"creditUsagePercent":20,"onDemandCap":{"val":1000},"onDemandUsed":{"val":250}}}"#.utf8)))
        let partial = try GrokClient.parse(.read(Data(#"{"config":{"creditUsagePercent":20,"onDemandCap":{"val":1000}}}"#.utf8)))
        let zero = try GrokClient.parse(.read(Data(#"{"config":{"creditUsagePercent":21,"onDemandCap":{},"onDemandUsed":{}}}"#.utf8)))
        let sequence = WalletQuotaSequence([known, partial, known, zero])
        let clock = now
        let provider = RetainedUsageProvider(provider: AdditionalUsageProvider(source: .grok,
            readQuota: { await sequence.next() }, readSessions: { _ in .init() }, history: QuotaHistoryStore(), clock: { clock }))
        await provider.refreshAccountUsage(historyHours: 24)
        let first = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertTrue(first.discoveredAgents.contains { $0.windowKey == "grok:extra" })
        await provider.refreshAccountUsage(historyHours: 24)
        let unknown = try await provider.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        let extraID = try XCTUnwrap(unknown.discoveredAgents.first { $0.windowKey == "grok:extra" }?.id)
        XCTAssertNil(unknown.snapshot(for: extraID)?.remainingPct)
        XCTAssertNil(ReportView(report: unknown, agents: first.discoveredAgents, settings: Settings(), now: now).rows.first { $0.id == extraID }?.level)
        await provider.refreshAccountUsage(historyHours: 24)
        let recovered = try await provider.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        XCTAssertEqual(recovered.snapshot(for: extraID)?.remainingPct, 75)
        await provider.refreshAccountUsage(historyHours: 24)
        let result = try await provider.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        XCTAssertFalse(result.discoveredAgents.contains { $0.windowKey == "grok:extra" })
        XCTAssertFalse(result.snapshots.contains { $0.agentId.hasSuffix("/grok:extra") })
        XCTAssertEqual(result.accounts?["Grok"]?.first?.wallets?.first?.limit, 0)
        let view = ReportView(report: result, agents: first.discoveredAgents, settings: Settings(), now: now)
        XCTAssertFalse(view.rows.contains { $0.agent.windowKey == "grok:extra" })
    }
}

private actor WalletQuotaSequence {
    private var values: [ProviderQuota]
    init(_ values: [ProviderQuota]) { self.values = values }
    func next() -> ProviderQuota { values.removeFirst() }
}
