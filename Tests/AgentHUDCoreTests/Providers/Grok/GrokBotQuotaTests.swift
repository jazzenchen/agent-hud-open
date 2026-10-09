import XCTest
@testable import AgentHUDCore

final class GrokBotQuotaTests: XCTestCase {
    func testPercentageUsesPercentUnitsAndKeepsObservationTime() throws {
        let f = try fixture()
        try writeQuota(f)
        let quota = try XCTUnwrap(GrokBotQuota.read(in: f.directory, now: f.now))
        XCTAssertEqual(try XCTUnwrap(quota.windows[0].remaining), 99.329611, accuracy: 0.000001)
        XCTAssertEqual(quota.plan, "SuperGrok")
        XCTAssertEqual(quota.observedAt, f.now.addingTimeInterval(-3600))
        XCTAssertEqual(quota.client, "Grok Bot")
        XCTAssertNil(quota.notice)
        XCTAssertNil(quota.displayNotice)
        XCTAssertNotNil(quota.sourceInfo)
    }

    func testAccountSwitchSignOutExpiryAndUnsupportedTeamCannotReuseCache() throws {
        let f = try fixture()
        try writeQuota(f)
        try f.writeAccount("another-account")
        XCTAssertNil(GrokBotQuota.read(in: f.directory, now: f.now))
        try f.writeAccount(nil)
        XCTAssertNil(GrokBotQuota.read(in: f.directory, now: f.now))
        try f.writeAccount(f.account)
        XCTAssertNil(GrokBotQuota.read(in: f.directory, now: f.now.addingTimeInterval(86400)))
        try writeQuota(f, team: 123)
        XCTAssertNil(GrokBotQuota.read(in: f.directory, now: f.now))
    }

    func testExtraBudgetIsSeparateAndFutureReadingIsRejected() throws {
        let f = try fixture()
        try writeQuota(f, extra: ["usedCents": 250, "limitCents": 1000])
        let quota = try XCTUnwrap(GrokBotQuota.read(in: f.directory, now: f.now))
        XCTAssertEqual(quota.windows.map(\.id), ["grok", "grok:extra"])
        XCTAssertEqual(quota.windows[1].remaining, 75)
        XCTAssertEqual(quota.wallets.first?.used, Decimal(string: "2.50"))
        XCTAssertEqual(quota.wallets.first?.limit, 10)
        XCTAssertEqual(quota.wallets.first?.observedAt, quota.observedAt)
        XCTAssertNil(GrokBotQuota.read(in: f.directory, now: f.now.addingTimeInterval(-7200)))
    }

    func testZeroWalletSurvivesMissingSubscriptionPercent() throws {
        let f = try fixture()
        var usage: [String: Any] = ["nextResetMs": NSNull(), "isSandTrial": false,
            "hasNonZeroIncludedLimit": true, "isTeamSeat": false,
            "onDemand": ["usedCents": 0, "limitCents": 0]]
        func parse() throws -> ProviderQuota? {
            let value: [String: Any] = ["kind": "present", "selectedTeamId": NSNull(),
                "expiresAtMs": f.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000,
                "reading": ["readAtMs": f.now.timeIntervalSince1970 * 1000, "usage": usage]]
            return GrokBotQuota.parse(try ProviderJSON.read(JSONSerialization.data(withJSONObject: value)), accountSlot: f.account, now: f.now)
        }
        let quota = try XCTUnwrap(parse())
        XCTAssertEqual(quota.windows.map(\.id), ["grok"])
        XCTAssertNil(quota.windows[0].remaining)
        XCTAssertEqual(quota.wallets.first?.used, 0)
        XCTAssertEqual(quota.wallets.first?.limit, 0)
        XCTAssertNil(quota.wallets.first?.usedPercent)
        XCTAssertEqual(quota.quotaWindowIDs, ["grok"])
        XCTAssertNotNil(quota.displayNotice)
        for extra in [["limitCents": 1000], ["usedCents": 125]] {
            usage["onDemand"] = extra
            let partial = try XCTUnwrap(parse())
            XCTAssertEqual(partial.windows.map(\.id), ["grok", "grok:extra"])
            XCTAssertEqual(partial.windows.map(\.remaining), [nil, nil])
            XCTAssertEqual(partial.windows[1].observedAt, f.now)
        }
        usage["hasNonZeroIncludedLimit"] = false
        XCTAssertNil(try parse(), "a wallet does not establish an included subscription limit")
    }

    func testFailedCLIUsesBotCacheAndUnlinkedCLIKeepsItsAccount() async throws {
        let f = try fixture(), now = Date()
        try writeQuota(f, now: now)
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let failed = try await GrokClient(home: home, botDirectory: f.directory).fetch()
        XCTAssertEqual(failed.client, "Grok Bot")
        let auth = ["https://auth.x.ai::fixture": ["key": "synthetic-token", "expires_at": "2035-01-01T00:00:00Z", "user_id": "fixture-user"]]
        try JSONSerialization.data(withJSONObject: auth).write(to: home.appendingPathComponent("auth.json"))
        let http = ProviderHTTP(send: { request in
            let answer = request.url?.path == "/v1/settings" ? "{\"subscription_tier_display\":\"SuperGrok\"}"
                : "{\"config\":{\"creditUsagePercent\":12,\"currentPeriod\":{\"type\":\"USAGE_PERIOD_TYPE_WEEKLY\"}}}"
            return Data(answer.utf8)
        })
        let online = try await GrokClient(home: home, http: http, botDirectory: f.directory).fetch()
        XCTAssertEqual(online.client, "Grok CLI")
        XCTAssertEqual(online.windows[0].remaining, 88)
        XCTAssertNotNil(online.observedAt)
        XCTAssertNotEqual(online.account?.id, failed.account?.id, "cache slots are not guessed to be CLI user ids")
        try f.writeAccount(nil)
        let rejected = GrokClient(home: home, http: ProviderHTTP(send: { _ in throw ProviderHTTPError(status: 401) }), botDirectory: f.directory)
        do { _ = try await rejected.fetch(); XCTFail("without a Bot account the CLI failure must remain visible") }
        catch { XCTAssertTrue(error is ProviderHTTPError) }
    }

    func testRepeatedCachePollsKeepSnapshotAccountAndHistoryTime() async throws {
        let f = try fixture()
        try writeQuota(f)
        let quota = try XCTUnwrap(GrokBotQuota.read(in: f.directory, now: f.now)), history = QuotaHistoryStore()
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { quota }, readSessions: { _ in .init() },
            history: history, clock: { f.now })
        await provider.refreshAccountUsage(historyHours: 24)
        await provider.refreshAccountUsage(historyHours: 24)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshots.first?.updatedAt, quota.observedAt)
        XCTAssertEqual(report.accounts?["Grok"]?.first?.observedAt, quota.observedAt)
        XCTAssertEqual(report.accounts?["Grok"]?.first?.client, "Grok Bot")
        XCTAssertEqual(report.quotaNotices, [:])
        let samples = await history.samples(agentId: report.snapshots[0].agentId, since: .distantPast)
        XCTAssertEqual(samples.count, 1)
        XCTAssertEqual(samples.first?.timestamp, quota.observedAt)
    }

    func testCancellationFromBillingAndSettingsIsNeverReplacedWithCache() async throws {
        let f = try fixture(), now = Date()
        try writeQuota(f, now: now)
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let auth = ["https://auth.x.ai::fixture": ["key": "synthetic-token", "expires_at": "2035-01-01T00:00:00Z"]]
        try JSONSerialization.data(withJSONObject: auth).write(to: home.appendingPathComponent("auth.json"))
        for cancelledPath in ["/v1/billing", "/v1/settings"] {
            let http = ProviderHTTP(send: { request in
                if request.url?.path == cancelledPath { throw CancellationError() }
                return Data("{\"config\":{\"creditUsagePercent\":12}}".utf8)
            })
            do {
                _ = try await GrokClient(home: home, http: http, botDirectory: f.directory).fetch()
                XCTFail("cancelling \(cancelledPath) must propagate")
            } catch { XCTAssertTrue(error is CancellationError) }
        }
    }

    func testExpiredCacheFailsAndSignOutMakesRetainedAccountHistorical() async throws {
        let f = try fixture()
        try writeQuota(f)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            try GrokBotQuota.fetch(in: f.directory, now: f.now) ?? ProviderQuota()
        }, readSessions: { _ in .init() }, history: QuotaHistoryStore(), clock: { f.now })
        let retained = RetainedUsageProvider(provider: provider)
        await retained.refreshAccountUsage(historyHours: 24)
        let first = try await retained.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(first.accounts?["Grok"]?.first?.isCurrent, true)
        try writeQuota(f, now: f.now.addingTimeInterval(-86400))
        await retained.refreshAccountUsage(historyHours: 24)
        let expired = try await retained.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        XCTAssertFalse(expired.vendorStatus("Grok").isNormal)
        XCTAssertEqual(expired.snapshots.first?.updatedAt, first.snapshots.first?.updatedAt)
        try f.writeAccount(nil)
        await retained.refreshAccountUsage(historyHours: 24)
        let signedOut = try await retained.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        XCTAssertEqual(signedOut.accounts?["Grok"]?.first?.isCurrent, false)
        XCTAssertEqual(signedOut.snapshots.first?.updatedAt, first.snapshots.first?.updatedAt)
    }

    func testQuotaFailureAndLocalNoticeStaySeparate() async throws {
        let f = try fixture()
        try writeQuota(f)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            try GrokBotQuota.fetch(in: f.directory, now: f.now) ?? ProviderQuota()
        }, readSessions: { _ in .init(notice: "Synthetic local history notice") }, history: QuotaHistoryStore(), clock: { f.now })
        let retained = RetainedUsageProvider(provider: provider)
        await retained.refreshAccountUsage(historyHours: 24)
        let first = try await retained.fetchUsage(agents: [], historyHours: 24)
        try writeQuota(f, now: f.now.addingTimeInterval(-86400))
        await retained.refreshAccountUsage(historyHours: 24)
        let failed = try await retained.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        let view = ReportView(report: failed, agents: failed.discoveredAgents, settings: Settings(), now: f.now)
        let section = try XCTUnwrap(view.accountSections(view.rows).first), account = try XCTUnwrap(section.account)
        XCTAssertEqual(view.accountSourceNotice(for: section), "Synthetic local history notice")
        XCTAssertNotNil(view.assessment(of: account).status.reason)
        XCTAssertEqual(view.accountNotice(for: section), view.assessment(of: account).status.reason! + " · Synthetic local history notice")
    }

    func testEmptyValuesDoNotClaimSignOutWithoutExplicitEvidence() async throws {
        let f = try fixture()
        try writeQuota(f)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            GrokBotQuota.read(in: f.directory, now: f.now) ?? ProviderQuota(displayNotice: "Connected without readable values")
        }, readSessions: { _ in .init() }, history: QuotaHistoryStore(), clock: { f.now })
        let retained = RetainedUsageProvider(provider: provider)
        await retained.refreshAccountUsage(historyHours: 24)
        let first = try await retained.fetchUsage(agents: [], historyHours: 24)
        try FileManager.default.removeItem(at: GrokBotCache.url(for: GrokBotCache.quotaKey(account: f.account), in: f.directory))
        await retained.refreshAccountUsage(historyHours: 24)
        let partial = try await retained.fetchUsage(agents: first.discoveredAgents, historyHours: 24)
        XCTAssertEqual(partial.accounts?["Grok"]?.first?.isCurrent, true)
        XCTAssertEqual(partial.snapshots.first?.updatedAt, first.snapshots.first?.updatedAt)
    }

    private func fixture() throws -> GrokBotCacheFixture {
        let f = try GrokBotCacheFixture()
        addTeardownBlock { try? FileManager.default.removeItem(at: f.directory) }
        return f
    }

    private func writeQuota(_ f: GrokBotCacheFixture, now: Date? = nil, team: Any = NSNull(), extra: Any = NSNull()) throws {
        let now = now ?? f.now
        let usage: [String: Any] = ["percentUsed": 0.670389, "nextResetMs": now.addingTimeInterval(3600).timeIntervalSince1970 * 1000,
            "isSandTrial": false, "hasNonZeroIncludedLimit": true, "isTeamSeat": false, "onDemand": extra, "grokPlanLabel": "SuperGrok"]
        try f.write(["kind": "present", "selectedTeamId": team, "expiresAtMs": now.addingTimeInterval(23 * 3600).timeIntervalSince1970 * 1000,
            "reading": ["readAtMs": now.addingTimeInterval(-3600).timeIntervalSince1970 * 1000, "usage": usage]],
            key: GrokBotCache.quotaKey(account: f.account), schema: 2)
    }
}
