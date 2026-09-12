import Foundation
import XCTest
@testable import AgentHUDCore

final class KimiQuotaIdentityTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    // Redacted shape of the live OAuth /me response: domain is omitted.
    private static let profile = #"{"user_id":"fixture-account","region":"REGION_CN"}"#
    private static let usage = #"{"usage":{"limit":"100","used":"10","resetTime":"2027-01-21T00:00:00Z"},"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"limit":"100","used":"20"}}],"membership":{"level":"Allegretto"}}"#

    private func credential(_ key: String, client: String = "Kimi", expiresAt: Date? = nil) -> OpenAgentCredential {
        OpenAgentCredentials.credential(.kimi, token: key, client: client, expiresAt: expiresAt)
    }
    private func directory() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }
    private func provider(_ input: Inputs, _ server: Server, identityCache: URL? = nil) -> OpenAgentUsageProvider {
        let client = OpenAgentQuotaClient(http: ProviderHTTP(send: { try await server.send($0) }))
        return OpenAgentUsageProvider(credentials: { input.credentials }, sessions: { _ in .init() },
            fetchQuota: { try await client.fetch($0, now: $1) }, history: QuotaHistoryStore(fileURL: nil),
            identify: { try await client.identify($0) }, clock: { input.now }, identityCacheURL: identityCache)
    }

    func testDifferentKeysWithSameOfficialIdentityFetchOnlyTwoWindowsOnce() async throws {
        let input = Inputs(now: now, credentials: [credential("first-key"), credential("second-key", client: "Pi")])
        let server = Server()
        let report = try await provider(input, server).fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshots.count, 2)
        XCTAssertEqual(Set(report.snapshots.compactMap(\.windowDuration)), [604800, 18000])
        XCTAssertEqual(report.activeQuotaPoolIDs?["Kimi"]?.count, 1)
        XCTAssertTrue(report.discoveredAgents.allSatisfy { $0.source == "Kimi, Pi" && $0.billingPool?.evidence == .account })
        let requests = await server.requests
        XCTAssertEqual(requests.filter { $0 == "me" }.count, 2)
        XCTAssertEqual(requests.filter { $0 == "usages" }.count, 1)
    }

    func testEachIdentityFieldAndDeploymentSeparatesAccounts() async throws {
        func identify(_ profile: String, service: OpenAgentCredential.Service = .kimi) async throws -> BillingPool {
            let client = OpenAgentQuotaClient(http: ProviderHTTP(send: { _ in Data(profile.utf8) }))
            return try await client.identify(OpenAgentCredentials.credential(service, token: "fixture-key", client: "Kimi")).pool
        }
        let canonical = try await identify(Self.profile)
        for profile in [#"{"user_id":"fixture-account","domain":0,"region":"REGION_CN"}"#,
                        #"{"user_id":"fixture-account","domain":"0","region":"REGION_CN"}"#,
                        #"{"user_id":"fixture-account","domain":null,"region":"REGION_CN"}"#] {
            let defaultDomain = try await identify(profile)
            XCTAssertEqual(canonical, defaultDomain)
        }
        for profile in [
            #"{"user_id":"other-account","domain":0,"region":"REGION_CN"}"#,
            #"{"user_id":"fixture-account","domain":1,"region":"REGION_CN"}"#,
            #"{"user_id":"fixture-account","domain":0,"region":"REGION_GLOBAL"}"#,
        ] {
            let other = try await identify(profile)
            XCTAssertNotEqual(canonical.id, other.id)
        }
        let global = try await identify(Self.profile, service: .kimiGlobal)
        XCTAssertNotEqual(canonical.id, global.id)
        for profile in [#"{"user_id":"fixture-account","domain":0}"#,
                        #"{"user_id":"","domain":1,"region":"REGION_CN"}"#,
                        #"{"user_id":"fixture-account","domain":1.5,"region":"REGION_CN"}"#,
                        #"{"user_id":"fixture-account","domain":"invalid","region":"REGION_CN"}"#] {
            do { _ = try await identify(profile); XCTFail("Incomplete identities must not merge") }
            catch { XCTAssertEqual(error.localizedDescription, ProviderFailure.format.localizedDescription) }
        }
    }

    @MainActor
    func testFailedIdentityStaysSeparateThenPromotionRemovesCredentialRows() async throws {
        let input = Inputs(now: now, credentials: [credential("first-key"), credential("second-key", client: "Pi")])
        let server = Server()
        await server.rejectProfile("second-key")
        let retained = RetainedUsageProvider(provider: provider(input, server))
        let first = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(first.snapshots.count, 4) // Identical quota values are not identity proof.
        XCTAssertNotNil(first.sourceNotices["Kimi"])
        XCTAssertEqual(Set(first.discoveredAgents.compactMap { $0.billingPool?.evidence }), [.account, .credential])
        let suite = "KimiQuotaIdentityTests.\(UUID())"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [])
        settings.mergeDiscovered(first.discoveredAgents, activeQuotaPoolIDs: first.activeQuotaPoolIDs)
        XCTAssertEqual(settings.agents.count, 4)
        await server.allowProfiles()
        input.advance(121)
        let second = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        settings.mergeDiscovered(second.discoveredAgents, activeQuotaPoolIDs: second.activeQuotaPoolIDs)
        XCTAssertEqual(second.snapshots.count, 2)
        XCTAssertEqual(second.discoveredAgents.count, 2)
        XCTAssertEqual(second.subscriptions.count, 1)
        XCTAssertEqual(second.consumerIdsByQuota.count, 2)
        XCTAssertEqual(settings.agents.count, 2)
        XCTAssertNil(second.sourceNotices["Kimi"])
    }

    func testVerifiedIdentitySurvivesRestartWithoutPersistingTokensOrProfile() async throws {
        let cache = try directory().appendingPathComponent("identities.json")
        let input = Inputs(now: now, credentials: [credential("first-key"), credential("second-key", client: "Pi")])
        let server = Server()
        let first = try await provider(input, server, identityCache: cache).fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        let stored = try String(contentsOf: cache, encoding: .utf8)
        for secret in ["first-key", "second-key", "fixture-account"] { XCTAssertFalse(stored.contains(secret)) }
        await server.rejectProfile("first-key")
        await server.rejectProfile("second-key")
        let second = try await provider(input, server, identityCache: cache).fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(second.snapshots, first.snapshots)
        XCTAssertEqual(second.activeQuotaPoolIDs, first.activeQuotaPoolIDs)
        XCTAssertNil(second.sourceNotices["Kimi"])
        let requests = await server.requests
        XCTAssertEqual(requests.filter { $0 == "me" }.count, 2)
    }

    @MainActor
    func testExpiryRetiresCachedQuotaAndSettingsBeforeQuotaCacheRefresh() async throws {
        let input = Inputs(now: now, credentials: [credential("expiring", expiresAt: now.addingTimeInterval(62))])
        let server = Server()
        let retained = RetainedUsageProvider(provider: provider(input, server))
        let suite = "KimiExpiryTests.\(UUID())"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SettingsStore(defaults: defaults, defaultAgents: [])
        let store = UsageStore(provider: retained, settings: settings)
        await retained.refreshAccountUsage(historyHours: UsageStore.historyHours)
        await store.refresh()
        XCTAssertEqual(store.rows.count, 2)
        input.advance(3)
        await retained.refreshAccountUsage(historyHours: UsageStore.historyHours)
        await store.refresh()
        XCTAssertTrue(store.rows.isEmpty)
        XCTAssertTrue(settings.agents.isEmpty)
        XCTAssertEqual(store.report?.activeQuotaPoolIDs?["Kimi"], [])
        XCTAssertTrue(store.report?.snapshots.isEmpty == true)
        XCTAssertTrue(store.report?.subscriptions.isEmpty == true)
        XCTAssertTrue(store.report?.services?.isEmpty == true)
        let requests = await server.requests
        XCTAssertEqual(requests.filter { $0 == "usages" }.count, 1)
    }

    func testTemporaryFailureRetainsActiveQuotaButUnauthorizedDoesNot() async throws {
        let input = Inputs(now: now, credentials: [credential("first-key")])
        let server = Server()
        let retained = RetainedUsageProvider(provider: provider(input, server))
        let good = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        await server.setUsageStatus(503)
        input.advance(121)
        let offline = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(offline.snapshots, good.snapshots)
        await server.setUsageStatus(401)
        input.advance(121)
        let invalid = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertTrue(invalid.snapshots.isEmpty)
        XCTAssertTrue(invalid.discoveredAgents.isEmpty)
        XCTAssertEqual(invalid.activeQuotaPoolIDs?["Kimi"], [])
    }

    func testExpiredAliasDoesNotHideValidAccountOrRetainExpiredClient() async throws {
        let input = Inputs(now: now, credentials: [credential("expiring", expiresAt: now.addingTimeInterval(62)),
                                                  credential("still-valid", client: "Pi")])
        let server = Server()
        let retained = RetainedUsageProvider(provider: provider(input, server))
        _ = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        input.advance(3)
        let report = try await retained.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshots.count, 2)
        XCTAssertEqual(report.activeQuotaPoolIDs?["Kimi"]?.count, 1)
        XCTAssertTrue(report.discoveredAgents.allSatisfy { $0.source == "Pi" })
        XCTAssertEqual(report.services?.map(\.client), ["Pi"])
    }

    func testRemovedCredentialsAndRestartDoNotResurrectOldRows() async throws {
        let input = Inputs(now: now, credentials: [credential("first-key"), credential("second-key", client: "Pi")])
        let server = Server()
        await server.rejectProfile("second-key")
        let cache = try directory().appendingPathComponent("report.json")
        let first = RetainedUsageProvider(provider: provider(input, server), cacheURL: cache)
        let old = try await first.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(old.snapshots.count, 4)
        input.removeCredentials()
        let restarted = RetainedUsageProvider(provider: provider(input, server), cacheURL: cache)
        XCTAssertFalse(restarted.initialReport?.snapshots.isEmpty ?? true)
        let fresh = try await restarted.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertTrue(fresh.snapshots.isEmpty)
        XCTAssertTrue(fresh.discoveredAgents.isEmpty)
        let saved = try JSONDecoder().decode(UsageReport.self, from: Data(contentsOf: cache))
        XCTAssertTrue(saved.snapshots.isEmpty)
    }

    func testOfflineRestartShowsCachedQuotaDuringCredentialRefresh() async throws {
        let cache = try directory().appendingPathComponent("report.json")
        let identityCache = cache.deletingLastPathComponent().appendingPathComponent("identities.json")
        let input = Inputs(now: now, credentials: [credential("first-key")])
        let server = Server()
        let first = RetainedUsageProvider(provider: provider(input, server, identityCache: identityCache), cacheURL: cache)
        let good = try await first.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        await server.setUsageStatus(503)
        let restarted = RetainedUsageProvider(provider: provider(input, server, identityCache: identityCache), cacheURL: cache)
        XCTAssertFalse(restarted.initialReport?.snapshots.isEmpty ?? true)
        let offline = try await restarted.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(offline.snapshots, good.snapshots)
    }

    func testExpiredJWTIsExcludedEvenWhenConfiguredAsAnAPIKey() throws {
        let payload = Data("{\"exp\":\(Int(now.timeIntervalSince1970 - 1)),\"user_id\":\"unverified\"}".utf8)
            .base64EncodedString().replacingOccurrences(of: "=", with: "").replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
        let token = "eyJhbGciOiJub25lIn0.\(payload).fixture"
        let discovered = OpenAgentCredentials.discover(home: try directory(), environment: ["KIMI_CODE_API_KEY": token], now: now)
        XCTAssertTrue(discovered.isEmpty)
        let unexpired = credential("eyJhbGciOiJub25lIn0.eyJleHAiOjE5OTk5OTk5OTksInVzZXJfaWQiOiJ1bnZlcmlmaWVkIn0.fixture")
        XCTAssertEqual(unexpired.pool.evidence, .credential)
    }

    private final class Inputs: @unchecked Sendable {
        private let lock = NSLock()
        private var date: Date
        private var values: [OpenAgentCredential]
        init(now: Date, credentials: [OpenAgentCredential]) { date = now; values = credentials }
        var now: Date { lock.withLock { date } }
        var credentials: [OpenAgentCredential] { lock.withLock { values } }
        func advance(_ seconds: TimeInterval) { lock.withLock { date.addTimeInterval(seconds) } }
        func removeCredentials() { lock.withLock { values = [] } }
    }
    private actor Server {
        private var rejectedProfiles: Set<String> = []
        private var usageStatus = 200
        var requests: [String] = []
        func rejectProfile(_ key: String) { rejectedProfiles.insert(key) }
        func allowProfiles() { rejectedProfiles = [] }
        func setUsageStatus(_ status: Int) { usageStatus = status }
        func send(_ request: URLRequest) throws -> Data {
            let path = request.url!.lastPathComponent
            requests.append(path)
            let key = request.value(forHTTPHeaderField: "Authorization")!.replacingOccurrences(of: "Bearer ", with: "")
            if path == "me" {
                if rejectedProfiles.contains(key) { throw ProviderHTTPError(status: 503) }
                return Data(KimiQuotaIdentityTests.profile.utf8)
            }
            guard path == "usages" else { throw ProviderFailure.format }
            if usageStatus != 200 { throw ProviderHTTPError(status: usageStatus) }
            return Data(KimiQuotaIdentityTests.usage.utf8)
        }
    }
}
