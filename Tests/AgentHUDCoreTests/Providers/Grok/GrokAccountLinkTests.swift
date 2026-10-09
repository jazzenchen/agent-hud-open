import AgentHUDSupport
import XCTest
@testable import AgentHUDCore

final class GrokAccountLinkTests: XCTestCase {
    func testInstalledConfirmedAccountCacheProbe() async throws {
        guard ProcessInfo.processInfo.environment["AGENT_HUD_PROBE_GROK_CACHE"] == "1" else {
            throw XCTSkip("Set AGENT_HUD_PROBE_GROK_CACHE=1 for a read-only native cache probe")
        }
        let home = FileManager.default.homeDirectoryForCurrentUser
        let links = home.appendingPathComponent("Library/Application Support/Agent HUD/grok-account-links.json")
        let client = GrokClient(http: ProviderHTTP(send: { _ in
            throw ProviderHTTPError(status: 500)
        }), accountLinksURL: links)
        let cache = try XCTUnwrap(GrokBotQuota.read(in: client.botDirectory, now: Date()))
        let quota = try await client.fetch()
        XCTAssertEqual(quota.client, "Grok Bot")
        XCTAssertEqual(quota.observedAt, cache.observedAt)
        XCTAssertEqual(quota.windows.first?.remaining, cache.windows.first?.remaining)
        XCTAssertEqual(quota.accountAliases, cache.account.map { [$0.id] })
        XCTAssertNotEqual(quota.account, cache.account)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { try await client.fetch() }, readSessions: { _ in .init() },
            history: QuotaHistoryStore())
        await provider.refreshAccountUsage(historyHours: 24)
        let current = try await provider.fetchUsage(agents: [], historyHours: 24)
        let previous = try JSONDecoder().decode(UsageReport.self, from: Data(contentsOf: home.appendingPathComponent("Library/Application Support/Agent HUD/last-usage-report.json")))
        let merged = current.retainingReadings(from: previous)
        XCTAssertEqual(merged.accounts?["Grok"]?.count, 1)
        XCTAssertEqual(merged.discoveredAgents.filter { $0.account?.provider == "Grok" }.count, quota.windows.count)
        print("Native Grok cache used: \(100 - (quota.windows.first?.remaining ?? 100))%; merged account count: \(merged.accounts?["Grok"]?.count ?? 0)")
    }

    func testConfirmedAccountUsesNativeCacheAndRetiresDuplicateRow() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let entry = try ProviderJSON.read(Data(#"{"key":"synthetic-token","expires_at":"2035-01-01T00:00:00Z","user_id":"fixture-user","email":"a@example.com"}"#.utf8))
        try JSONEncoder().encode(ProviderJSON.object(["https://auth.x.ai::fixture": entry])).write(to: home.appendingPathComponent("auth.json"))
        let cli = try XCTUnwrap(GrokClient.account(entry))
        let bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: cli.id]).write(to: links)
        let observed = f.now.addingTimeInterval(-60)
        let cache = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6)], plan: "SuperGrok", account: bot,
            observedAt: observed, client: "Grok Bot")
        let at = f.now
        let client = GrokClient(home: home, http: ProviderHTTP(send: { request in
            XCTAssertEqual(request.url?.path, "/v1/billing", "wallet enrichment must not fetch settings")
            XCTAssertEqual(request.timeoutInterval, 2)
            return Data(#"{"config":{"creditUsagePercent":4,"prepaidBalance":{"val":1446}}}"#.utf8)
        }), botDirectory: f.directory, clock: { at }, readBot: { cache }, accountLinksURL: links)
        let quota = try await client.fetch()
        XCTAssertEqual(quota.account, cli)
        XCTAssertEqual(quota.accountAliases, [bot.id])
        XCTAssertEqual(quota.observedAt, observed)
        XCTAssertEqual(quota.windows.first?.remaining, 62.6)
        XCTAssertEqual(quota.label, "a@example.com")
        XCTAssertEqual(quota.wallets.first?.balance, Decimal(string: "14.46"))
        XCTAssertEqual(quota.wallets.first?.observedAt, at)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { try await client.fetch() }, readSessions: { _ in .init() },
            history: QuotaHistoryStore(), clock: { at })
        await provider.refreshAccountUsage(historyHours: 24)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        let oldRows = [cli, bot].map { AgentDescriptor(id: $0.windowID("grok"), vendor: "Grok", model: "Weekly", source: "", enabled: true, account: $0) }
        let old = UsageReport(generatedAt: observed, snapshots: oldRows.map { .init(agentId: $0.id, remainingPct: 96, updatedAt: observed) },
            sessions: [], discoveredAgents: oldRows, accounts: ["Grok": [cli, bot].map { .init(account: $0, observedAt: observed) }])
        let merged = report.retainingReadings(from: old)
        XCTAssertEqual(merged.snapshots.map(\.agentId), [cli.windowID("grok")])
        XCTAssertEqual(merged.accounts?["Grok"]?.map(\.account), [cli])
        XCTAssertEqual(merged.snapshots.first?.updatedAt, observed)
    }

    func testLinkForAnotherLoginDoesNotMergeItsCache() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data(#"{"https://auth.x.ai::fixture":{"key":"synthetic-token","expires_at":"2035-01-01T00:00:00Z","user_id":"other-user"}}"#.utf8)
            .write(to: home.appendingPathComponent("auth.json"))
        let bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: "other-account-id"]).write(to: links)
        let cache = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6)], account: bot, observedAt: f.now, client: "Grok Bot")
        let at = f.now
        let http = ProviderHTTP(send: { request in Data((request.url?.path == "/v1/settings" ? "{}" : #"{"config":{"creditUsagePercent":4}}"#).utf8) })
        let quota = try await GrokClient(home: home, http: http, botDirectory: f.directory, clock: { at }, readBot: { cache }, accountLinksURL: links).fetch()
        XCTAssertEqual(quota.client, "Grok CLI")
        XCTAssertEqual(quota.windows.first?.remaining, 96)
        XCTAssertNil(quota.accountAliases)
    }

    func testConfirmedIdentitySurvivesExpiredOrMissingCLIToken() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let account = try XCTUnwrap(ProviderAccount.identified(provider: "Grok", user: "fixture-user", workspace: nil, evidence: .credential))
        let bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: account.id]).write(to: links)
        let observed = f.now.addingTimeInterval(-60), at = f.now
        let cache = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6)], account: bot,
            observedAt: observed, client: "Grok Bot")
        let client = GrokClient(home: home, http: ProviderHTTP(send: { _ in
            XCTFail("a valid native cache does not need a CLI token"); throw ProviderHTTPError(status: 500)
        }), botDirectory: f.directory, clock: { at }, readBot: { cache }, accountLinksURL: links)
        for entry in [
            ["user_id": "fixture-user", "key": "synthetic-token", "expires_at": "2020-01-01T00:00:00Z"],
            ["user_id": "fixture-user"]
        ] {
            try JSONSerialization.data(withJSONObject: ["https://auth.x.ai::fixture": entry]).write(to: home.appendingPathComponent("auth.json"))
            let quota = try await client.fetch()
            XCTAssertEqual(quota.account, account)
            XCTAssertEqual(quota.accountAliases, [bot.id])
            XCTAssertEqual(quota.observedAt, observed)
            XCTAssertEqual(quota.windows.first?.remaining, 62.6)
        }
    }

    func testUserPrincipalWithTeamIDUsesConfirmedNativeCache() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let account = try XCTUnwrap(ProviderAccount.identified(provider: "Grok", user: "fixture-user", workspace: "fixture-team", evidence: .credential))
        let bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: account.id]).write(to: links)
        let observed = f.now.addingTimeInterval(-60), at = f.now
        let cache = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 61.8)], account: bot,
            observedAt: observed, client: "Grok Bot")
        for principal in ["User", "Team"] {
            let entry = ["user_id": "fixture-user", "principal_type": principal, "team_id": "fixture-team"]
            try JSONSerialization.data(withJSONObject: ["https://auth.x.ai::fixture": entry]).write(to: home.appendingPathComponent("auth.json"))
            let quota = try await GrokClient(home: home, http: ProviderHTTP(send: { _ in
                XCTFail("the native cache does not need a CLI request"); throw ProviderHTTPError(status: 500)
            }), botDirectory: f.directory, clock: { at }, readBot: { cache }, accountLinksURL: links).fetch()
            XCTAssertEqual(quota.account, principal == "User" ? account : bot)
            XCTAssertEqual(quota.accountAliases, principal == "User" ? [bot.id] : nil)
            XCTAssertEqual(quota.observedAt, observed)
            XCTAssertEqual(quota.windows.first?.remaining, 61.8)
            XCTAssertEqual(quota.client, "Grok Bot")
        }
    }

    func testAmbiguousTeamOrMissingIdentityCannotConfirmPersonalCache() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let account = try XCTUnwrap(ProviderAccount.identified(provider: "Grok", user: "fixture-user", workspace: nil, evidence: .credential))
        let bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: account.id]).write(to: links)
        let at = f.now
        let cache = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6)], account: bot, observedAt: at, client: "Grok Bot")
        let client = GrokClient(home: home, http: ProviderHTTP(send: { _ in
            XCTFail("expired credentials cannot start a CLI request"); throw ProviderHTTPError(status: 500)
        }), botDirectory: f.directory, clock: { at }, readBot: { cache }, accountLinksURL: links)
        let loginRecords: [[String: [String: String]]] = [
            ["https://auth.x.ai::one": ["user_id": "fixture-user"], "https://auth.x.ai::two": ["user_id": "another-user"]],
            ["https://auth.x.ai::one": ["user_id": "fixture-user", "principal_type": "team"]],
            ["https://auth.x.ai::one": ["user_id": "fixture-user", "team_id": "fixture-team"]],
            ["https://auth.x.ai::one": ["user_id": "fixture-user"], "https://accounts.x.ai/sign-in": [:]]
        ]
        for records in loginRecords {
            try JSONSerialization.data(withJSONObject: records).write(to: home.appendingPathComponent("auth.json"))
            let quota = try await client.fetch()
            XCTAssertEqual(quota.account, bot)
            XCTAssertNil(quota.accountAliases)
        }
    }

    func testNativeQuotaFileChangesRefreshWithoutWaitingForAccountPoll() async throws {
        let f = try GrokBotCacheFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        func write(used: Double, readAt: Date) throws -> URL {
            try f.write(["kind": "present", "selectedTeamId": NSNull(), "expiresAtMs": f.now.addingTimeInterval(3600).timeIntervalSince1970 * 1000,
                "reading": ["readAtMs": readAt.timeIntervalSince1970 * 1000, "usage": ["percentUsed": used, "nextResetMs": NSNull(),
                "isSandTrial": false, "hasNonZeroIncludedLimit": true, "isTeamSeat": false]]], key: GrokBotCache.quotaKey(account: f.account), schema: 2)
        }
        _ = try write(used: 4, readAt: f.now.addingTimeInterval(-3600))
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            try GrokBotQuota.fetch(in: f.directory, now: f.now) ?? ProviderQuota()
        }, readSessions: { _ in .init() }, history: QuotaHistoryStore(), clock: { f.now }, botQuotaDirectory: f.directory)
        await provider.refreshAccountUsage(historyHours: 24)
        let file = try write(used: 37.4, readAt: f.now)
        await provider.fileChanges([file.path])
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshots.first?.remainingPct, 62.6)
        XCTAssertEqual(report.snapshots.first?.updatedAt, f.now)
    }
}
