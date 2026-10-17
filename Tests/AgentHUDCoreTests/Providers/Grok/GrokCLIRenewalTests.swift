import XCTest
@testable import AgentHUDCore

final class GrokCLIRenewalTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private final class Calls: @unchecked Sendable {
        var renewals = 0
        var tokens: [String] = []
    }

    private func login(key: String, expires: Date, refresh: String? = "fixture-refresh") -> String {
        let refresh = refresh.map { #","refresh_token":"\#($0)""# } ?? ""
        return #"{"https://auth.x.ai::cli":{"key":"\#(key)","user_id":"fixture-user","principal_type":"User","expires_at":"\#(ISO8601DateFormatter().string(from: expires))"\#(refresh)}}"#
    }

    private func client(home: URL, calls: Calls, renew: @escaping @Sendable () -> Bool) -> GrokClient {
        let now = now
        return GrokClient(home: home, http: ProviderHTTP(send: { request in
            if request.url?.path == "/v1/billing" {
                let token = request.value(forHTTPHeaderField: "Authorization") ?? ""
                calls.tokens.append(token)
                guard token == "Bearer renewed" else { throw ProviderHTTPError(status: 401) }
                return Data(#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY"},"creditUsagePercent":8.0}}"#.utf8)
            }
            return Data("{}".utf8)
        }), botDirectory: home.appendingPathComponent("no-bot"), clock: { now }, readBot: { nil }, renewCLI: {
            calls.renewals += 1
            return renew()
        })
    }

    private func home() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    func testAnExpiredLoginIsRenewedByTheCLIAndReadAgain() async throws {
        let root = try home(), auth = root.appendingPathComponent("auth.json"), calls = Calls()
        try Data(login(key: "expired", expires: now.addingTimeInterval(-4 * 86400)).utf8).write(to: auth)
        let renewed = Data(login(key: "renewed", expires: now.addingTimeInterval(6 * 3600)).utf8)
        let quota = try await client(home: root, calls: calls) {
            try? renewed.write(to: auth)
            return true
        }.fetch()
        XCTAssertEqual(calls.renewals, 1)
        XCTAssertEqual(calls.tokens, ["Bearer renewed"], "The expired token is never sent")
        XCTAssertEqual(quota.windows.first?.remaining, 92)
    }

    func testAValidLoginDoesNotStartTheCLI() async throws {
        let root = try home(), calls = Calls()
        try Data(login(key: "renewed", expires: now.addingTimeInterval(3600)).utf8).write(to: root.appendingPathComponent("auth.json"))
        _ = try await client(home: root, calls: calls) { XCTFail("not renewed"); return false }.fetch()
        XCTAssertEqual(calls.renewals, 0)
    }

    func testAFailedOrImpossibleRenewalStaysSignedOut() async throws {
        let root = try home(), auth = root.appendingPathComponent("auth.json"), calls = Calls()
        try Data(login(key: "expired", expires: now.addingTimeInterval(-60)).utf8).write(to: auth)
        do {
            _ = try await client(home: root, calls: calls) { false }.fetch()
            XCTFail("An unrenewed login has no quota")
        } catch {}
        XCTAssertEqual(calls.renewals, 1)

        try Data(login(key: "expired", expires: now.addingTimeInterval(-60), refresh: nil).utf8).write(to: auth)
        do {
            _ = try await client(home: root, calls: calls) { true }.fetch()
            XCTFail("A login without a refresh token cannot be renewed")
        } catch {}
        XCTAssertEqual(calls.renewals, 1, "Only a renewable login starts the CLI")
        XCTAssertTrue(calls.tokens.isEmpty)
    }

    func testFailedRenewalsDoubleThePauseUpToItsLimitAndARenewalResetsIt() {
        var schedule = GrokCLIRenewal.Schedule()
        XCTAssertTrue(schedule.begin(at: now))
        XCTAssertFalse(schedule.begin(at: now.addingTimeInterval(899)), "No second attempt inside 15 minutes")
        schedule.finish(renewed: false)
        XCTAssertFalse(schedule.begin(at: now.addingTimeInterval(1_799)))
        XCTAssertTrue(schedule.begin(at: now.addingTimeInterval(1_800)), "A failure doubles the pause to 30 minutes")

        for _ in 0..<10 { schedule.finish(renewed: false) }
        XCTAssertEqual(schedule.pause, GrokCLIRenewal.longestPause)
        XCTAssertFalse(schedule.begin(at: now.addingTimeInterval(1_800 + 4 * 3600 - 1)))
        XCTAssertTrue(schedule.begin(at: now.addingTimeInterval(1_800 + 4 * 3600)))

        schedule.finish(renewed: true)
        XCTAssertEqual(schedule.pause, GrokCLIRenewal.retryInterval)
    }

    func testOnlyTheInstalledGrokCLIIsStarted() {
        let home = URL(fileURLWithPath: "/Users/fixture")
        XCTAssertEqual(GrokCLIRenewal.candidates(home: home, grokHome: home.appendingPathComponent(".grok")).map(\.path),
                       ["/Users/fixture/.grok/bin/grok"])
        XCTAssertEqual(GrokCLIRenewal.candidates(home: home, grokHome: URL(fileURLWithPath: "/Volumes/grok")).map(\.path),
                       ["/Volumes/grok/bin/grok", "/Users/fixture/.grok/bin/grok"],
                       "Never `PATH`, Homebrew or ~/.local/bin, where another program named grok may be")
    }

    func testRenewableNeedsEveryLoginExpiredAndOneRefreshToken() throws {
        func entries(_ text: String) throws -> [ProviderJSON] {
            try ProviderJSON.read(Data(text.utf8)).objectValue.map { Array($0.values) } ?? []
        }
        XCTAssertTrue(GrokCLIRenewal.isRenewable(try entries(login(key: "k", expires: now.addingTimeInterval(-1))), now: now))
        XCTAssertFalse(GrokCLIRenewal.isRenewable(try entries(login(key: "k", expires: now.addingTimeInterval(60))), now: now))
        XCTAssertFalse(GrokCLIRenewal.isRenewable(try entries(login(key: "k", expires: now.addingTimeInterval(-1), refresh: nil)), now: now))
        XCTAssertFalse(GrokCLIRenewal.isRenewable([], now: now))
    }
}
