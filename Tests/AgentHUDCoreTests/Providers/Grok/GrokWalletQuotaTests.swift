import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

final class GrokWalletQuotaTests: XCTestCase {
    private func json(_ text: String) throws -> ProviderJSON { try .read(Data(text.utf8)) }

    func testUnifiedBillingWithZeroWalletsKeepsWeeklyUsageUnknown() throws {
        let quota = try GrokClient.parse(json(#"{"config":{"isUnifiedBillingUser":true,"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-09-01T00:00:00Z","end":"2026-09-08T00:00:00Z"},"prepaidBalance":{},"onDemandCap":{"val":0},"onDemandUsed":{}}}"#))
        XCTAssertEqual(quota.windows.map(\.id), ["grok"])
        XCTAssertNil(quota.windows[0].remaining)
        XCTAssertEqual(quota.windows[0].label, "Weekly usage limit")
        XCTAssertEqual(quota.windows[0].duration, 604800)
        XCTAssertEqual(quota.windows[0].reset, DateParsing.internet("2026-09-08T00:00:00Z"))
        XCTAssertEqual(quota.wallets, [AccountWallet(kind: .prepaid, balance: 0), AccountWallet(kind: .onDemand, used: 0, limit: 0)])
        XCTAssertNil(quota.wallets[1].usedPercent)
        XCTAssertEqual(quota.quotaWindowIDs, ["grok"], "an explicit zero limit retires any retained Extra percentage")
        XCTAssertNotNil(quota.displayNotice)
        XCTAssertNil(quota.notice)
    }

    func testWalletCentsAreExactAndIndependentFromSubscription() throws {
        let quota = try GrokClient.parse(json(#"{"config":{"creditUsagePercent":12.5,"prepaidBalance":{"val":"9007199254740993"},"onDemandCap":{"val":5000},"onDemandUsed":{"val":300}}}"#))
        XCTAssertEqual(quota.windows.map(\.remaining), [87.5, 94])
        XCTAssertEqual(quota.wallets[0].balance, Decimal(string: "90071992547409.93"))
        XCTAssertEqual(quota.wallets[1].used, 3)
        XCTAssertEqual(quota.wallets[1].limit, 50)
        XCTAssertEqual(quota.wallets[1].usedPercent, 6)
        XCTAssertNil(quota.displayNotice)
        XCTAssertNil(quota.sourceInfo)
    }

    func testMissingAndMalformedWalletsDoNotBecomeZeroOrDiscardQuota() throws {
        for wallet in ["null", "42", #"{"val":null}"#, #"{"val":true}"#, #"{"val":-1}"#,
                       #"{"val":1.5}"#, #"{"val":"invalid"}"#, #"{"unexpected":0}"#] {
            let quota = try GrokClient.parse(json("{\"config\":{\"creditUsagePercent\":25,\"prepaidBalance\":\(wallet),\"onDemandCap\":\(wallet),\"onDemandUsed\":\(wallet)}}"))
            XCTAssertEqual(quota.windows.map(\.remaining), [75], wallet)
            XCTAssertEqual(quota.wallets, [], wallet)
        }
        XCTAssertEqual(try GrokClient.parse(json(#"{"config":{"creditUsagePercent":25}}"#)).wallets, [])
        let partial = try GrokClient.parse(json(#"{"config":{"onDemandUsed":{"val":125}}}"#))
        XCTAssertEqual(partial.wallets, [AccountWallet(kind: .onDemand, used: Decimal(string: "1.25"))])
        XCTAssertNil(partial.wallets[0].usedPercent)
        XCTAssertEqual(partial.windows.map(\.id), ["grok", "grok:extra"])
        XCTAssertEqual(partial.windows.map(\.remaining), [nil, nil])
        let limitOnly = try GrokClient.parse(json(#"{"config":{"creditUsagePercent":25,"onDemandCap":{"val":1000}}}"#))
        XCTAssertEqual(limitOnly.windows.map(\.id), ["grok", "grok:extra"])
        XCTAssertEqual(limitOnly.windows.map(\.remaining), [75, nil])
    }

    func testMissingNullAndInvalidPercentageRemainUnknownButExplicitZeroIsKnown() throws {
        for percent in ["null", "-1", #""not-a-percentage""#] {
            let quota = try GrokClient.parse(json("{\"config\":{\"creditUsagePercent\":\(percent),\"currentPeriod\":{\"type\":\"USAGE_PERIOD_TYPE_MONTHLY\"}}}"))
            XCTAssertNil(quota.windows[0].remaining)
            XCTAssertEqual(quota.windows[0].label, "Monthly usage limit")
            XCTAssertNotNil(quota.displayNotice)
        }
        let zero = try GrokClient.parse(json(#"{"config":{"creditUsagePercent":0}}"#))
        XCTAssertEqual(zero.windows[0].remaining, 100)
        XCTAssertNil(zero.displayNotice)
    }

    func testConfirmedNativeQuotaKeepsItsTimeAndUsesLatestWalletObservation() async throws {
        let f = try linkedFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let nativeTime = f.now.addingTimeInterval(-60)
        var native = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6),
            .init(id: "grok:extra", label: "Extra usage", remaining: 90)],
            account: f.bot, observedAt: nativeTime, client: "Grok Bot")
        native.sourceInfo = "Native cache source"
        native.wallets = [AccountWallet(kind: .onDemand, used: 1, limit: 10, observedAt: nativeTime)]
        let cache = native, at = f.now
        let quota = try await GrokClient(home: f.home, http: ProviderHTTP(send: { request in
            XCTAssertEqual(request.url?.path, "/v1/billing")
            XCTAssertEqual(request.timeoutInterval, 2)
            return Data(#"{"config":{"creditUsagePercent":4,"prepaidBalance":{"val":1446},"onDemandCap":{"val":0},"onDemandUsed":{"val":0}}}"#.utf8)
        }), clock: { at }, readBot: { cache }, accountLinksURL: f.links).fetch()
        XCTAssertEqual(quota.client, "Grok Bot")
        XCTAssertEqual(quota.windows[0].remaining, 62.6)
        XCTAssertEqual(quota.observedAt, nativeTime)
        XCTAssertEqual(quota.sourceInfo, "Native cache source")
        XCTAssertEqual(quota.windows.map(\.id), ["grok"], "a newer zero-dollar limit cannot keep an older extra percentage")
        XCTAssertEqual(quota.quotaWindowIDs, ["grok"])
        XCTAssertEqual(quota.wallets.count, 2)
        XCTAssertEqual(quota.wallets.first(where: { $0.kind == .onDemand }), AccountWallet(kind: .onDemand, used: 0, limit: 0, observedAt: at))
        XCTAssertEqual(quota.wallets.first(where: { $0.kind == .prepaid }), AccountWallet(kind: .prepaid, balance: Decimal(string: "14.46"), observedAt: at))
    }

    func testLatestWalletUpdatesExtraWindowWithoutRefreshingNativeSubscription() async throws {
        let f = try linkedFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let nativeTime = f.now.addingTimeInterval(-60), reset = f.now.addingTimeInterval(3600)
        var native = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6),
            .init(id: "grok:extra", label: "Extra usage", remaining: 90, reset: reset)],
            account: f.bot, observedAt: nativeTime, client: "Grok Bot")
        native.wallets = [AccountWallet(kind: .onDemand, used: 1, limit: 10, observedAt: nativeTime)]
        let cache = native, at = f.now
        let quota = try await GrokClient(home: f.home, http: ProviderHTTP(send: { _ in
            Data(#"{"config":{"creditUsagePercent":4,"onDemandCap":{"val":5000},"onDemandUsed":{"val":300}}}"#.utf8)
        }), clock: { at }, readBot: { cache }, accountLinksURL: f.links).fetch()
        XCTAssertEqual(quota.client, "Grok Bot")
        XCTAssertEqual(quota.observedAt, nativeTime)
        XCTAssertEqual(quota.windows[0].remaining, 62.6)
        XCTAssertEqual(quota.windows[1].remaining, 94)
        XCTAssertEqual(quota.windows[1].observedAt, at)
        XCTAssertNil(quota.windows[1].reset, "the new wallet cannot claim the native cache's different reset")
        XCTAssertEqual(quota.wallets.first?.observedAt, at)
    }

    func testWalletFetchFailureKeepsNativeReadingButCancellationPropagates() async throws {
        let f = try linkedFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let native = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6)],
            account: f.bot, observedAt: f.now, client: "Grok Bot")
        let at = f.now
        let failed = try await GrokClient(home: f.home, http: ProviderHTTP(send: { _ in throw ProviderHTTPError(status: 401) }),
            clock: { at }, readBot: { native }, accountLinksURL: f.links).fetch()
        XCTAssertEqual(failed.client, "Grok Bot")
        XCTAssertEqual(failed.windows[0].remaining, 62.6)
        XCTAssertEqual(failed.observedAt, at)
        do {
            _ = try await GrokClient(home: f.home, http: ProviderHTTP(send: { _ in throw CancellationError() }),
                clock: { at }, readBot: { native }, accountLinksURL: f.links).fetch()
            XCTFail("wallet enrichment cancellation must propagate")
        } catch { XCTAssertTrue(error is CancellationError) }
    }

    func testIncompleteCLIWalletReplacesNativeExtraPercentageWithUnknown() async throws {
        let f = try linkedFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        let nativeTime = f.now.addingTimeInterval(-60)
        var native = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: 62.6),
            .init(id: "grok:extra", label: "Extra usage", remaining: 90)],
            account: f.bot, observedAt: nativeTime, client: "Grok Bot")
        native.wallets = [AccountWallet(kind: .onDemand, used: 1, limit: 10, observedAt: nativeTime)]
        let cache = native, at = f.now
        let quota = try await GrokClient(home: f.home, http: ProviderHTTP(send: { _ in
            Data(#"{"config":{"creditUsagePercent":4,"onDemandCap":{"val":5000}}}"#.utf8)
        }), clock: { at }, readBot: { cache }, accountLinksURL: f.links).fetch()
        XCTAssertEqual(quota.windows.map(\.id), ["grok", "grok:extra"])
        XCTAssertEqual(quota.windows.map(\.remaining), [62.6, nil])
        XCTAssertEqual(quota.windows[1].observedAt, at)
        XCTAssertEqual(quota.observedAt, nativeTime)
    }

    func testReadableCLIQuotaCanReplaceConfirmedUnknownNativeQuota() async throws {
        let f = try linkedFixture()
        defer { try? FileManager.default.removeItem(at: f.directory) }
        var native = ProviderQuota(windows: [.init(id: "grok", label: "Weekly", remaining: nil)],
            account: f.bot, observedAt: f.now.addingTimeInterval(-60), client: "Grok Bot")
        native.displayNotice = "Native quota unavailable"
        native.sourceInfo = "Native cache source"
        let cache = native, at = f.now
        let quota = try await GrokClient(home: f.home, http: ProviderHTTP(send: { _ in
            Data(#"{"config":{"creditUsagePercent":10,"prepaidBalance":{"val":0}}}"#.utf8)
        }), clock: { at }, readBot: { cache }, accountLinksURL: f.links).fetch()
        XCTAssertEqual(quota.client, "Grok CLI")
        XCTAssertEqual(quota.windows[0].remaining, 90)
        XCTAssertEqual(quota.observedAt, at)
        XCTAssertNil(quota.displayNotice)
        XCTAssertNil(quota.sourceInfo)
        XCTAssertEqual(quota.accountAliases, [f.bot.id])
    }

    private func linkedFixture() throws -> (directory: URL, home: URL, links: URL, bot: ProviderAccount, now: Date) {
        let f = try GrokBotCacheFixture()
        let home = f.directory.appendingPathComponent("cli")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let entry = try json(#"{"key":"synthetic-token","expires_at":"2035-01-01T00:00:00Z","user_id":"fixture-user"}"#)
        try JSONEncoder().encode(ProviderJSON.object(["https://auth.x.ai::fixture": entry])).write(to: home.appendingPathComponent("auth.json"))
        let cli = try XCTUnwrap(GrokClient.account(entry)), bot = ProviderAccount.unresolved(provider: "Grok", home: "fixture-bot")
        let links = f.directory.appendingPathComponent("links.json")
        try JSONSerialization.data(withJSONObject: [bot.id: cli.id]).write(to: links)
        return (f.directory, home, links, bot, f.now)
    }
}
