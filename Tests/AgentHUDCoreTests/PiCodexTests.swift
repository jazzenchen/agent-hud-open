import Foundation
import XCTest
@testable import AgentHUDCore

final class PiCodexTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private static let payload = #"{"account_id":"workspace","email":"A@Example.com","plan_type":"plus","rate_limit":{"primary_window":{"used_percent":20,"limit_window_seconds":18000,"reset_at":1800018000},"secondary_window":{"used_percent":30,"limit_window_seconds":604800,"reset_at":1800604800}},"additional_rate_limits":[{"metered_feature":"base_model_inference","limit_name":"gpt-reserve","rate_limit":{"primary_window":{"used_percent":0,"limit_window_seconds":604800,"reset_at":1800604800}}}],"rate_limit_reset_credits":{"available_count":2}}"#

    func testBackendMappingSharesNativeAccountAndWindowIds() throws {
        let limits = try PiCodexClient.parse(Data(Self.payload.utf8), expectedAccount: "workspace")
        let native = try JSONDecoder().decode(CodexRateLimits.self, from: Data(#"{"accountId":"workspace","account":{"email":"a@example.com"},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":20,"windowDurationMins":300,"resetsAt":1800018000}}}}"#.utf8))
        XCTAssertEqual(limits.providerAccount(home: "pi"), native.providerAccount(home: ""))
        XCTAssertEqual(limits.rows(home: "pi").first?.id, native.rows(home: "").first?.id)
        XCTAssertEqual(limits.rows.map(\.label), ["5h", "Weekly", "Luna Reserve · Weekly"])
        XCTAssertEqual(limits.rows.first?.window.remainingPct, 80)
        XCTAssertEqual(limits.rateLimitResetCredits?.availableCount, 2)
        XCTAssertThrowsError(try PiCodexClient.parse(Data(Self.payload.utf8), expectedAccount: "another-workspace"))
        XCTAssertThrowsError(try PiCodexClient.parse(Data("{}".utf8), expectedAccount: "workspace"))
        XCTAssertThrowsError(try PiCodexClient.parse(Data(Self.payload.replacingOccurrences(of: "\"used_percent\":20,", with: "").utf8), expectedAccount: "workspace"))
    }

    func testReadsOnlyPiAccessTokenAndDoesNotMutateCredentials() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("auth.json")
        let content = #"{"openai-codex":{"type":"oauth","access":"test-access","refresh":"never-use","accountId":"workspace","expires":1800100000000}}"#
        try content.write(to: file, atomically: true, encoding: .utf8)
        let client = PiCodexClient(directory: dir, http: ProviderHTTP(send: { request in
            XCTAssertEqual(request.url?.absoluteString, "https://chatgpt.com/backend-api/wham/usage")
            XCTAssertEqual(request.httpMethod, "GET")
            XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer test-access")
            XCTAssertEqual(request.value(forHTTPHeaderField: "ChatGPT-Account-Id"), "workspace")
            XCTAssertNil(request.httpBody)
            return Data(Self.payload.utf8)
        }))
        let value = try await client.fetch(now: now)
        XCTAssertEqual(value?.account?.email, "A@Example.com")
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), content)
        do { _ = try await client.fetch(now: now.addingTimeInterval(100_001)); XCTFail("expired token must stay with Pi") }
        catch { XCTAssertTrue(error.localizedDescription.contains("Pi")) }
        try "{}".write(to: file, atomically: true, encoding: .utf8)
        let absent = try await client.fetch(now: now)
        XCTAssertNil(absent)
    }

    func testSameAccountHasOneSetOfWindowsAndDistinctAccountsStaySeparate() async throws {
        let a = try PiCodexClient.parse(Data(Self.payload.utf8), expectedAccount: "workspace")
        let b = try PiCodexClient.parse(Data(Self.payload.replacingOccurrences(of: "A@Example.com", with: "b@example.com").utf8), expectedAccount: "workspace")
        for (pi, expected) in [(a, 1), (b, 2)] {
            let provider = CodexUsageProvider(readLimits: { a }, transcripts: CodexTranscriptStore(roots: []),
                history: QuotaHistoryStore(), clock: { [now] in now }, readPiLimits: { pi })
            await provider.refreshAccountUsage(historyHours: 24)
            let report = try await provider.fetchUsage(agents: [], historyHours: 24)
            XCTAssertEqual(report.accounts?["Codex"]?.count, expected)
            XCTAssertEqual(report.snapshots.count, expected * 3)
            XCTAssertEqual(Set(report.snapshots.map(\.agentId)).count, report.snapshots.count)
            for account in report.accounts?["Codex"] ?? [] {
                XCTAssertEqual(report.resetCredits(for: account.account.id)?.availableCount, 2)
            }
            if expected == 2 { XCTAssertNil(report.codexResetCredits, "legacy unscoped credits cannot describe two accounts") }
        }
    }

    func testPiFailureDoesNotSuppressNativeResetAndFailedWindowDoesNotNotify() async throws {
        let a = try PiCodexClient.parse(Data(Self.payload.utf8), expectedAccount: "workspace")
        let b = try PiCodexClient.parse(Data(Self.payload.replacingOccurrences(of: "A@Example.com", with: "b@example.com").utf8), expectedAccount: "workspace")
        let steps = Steps([.success(b), .failure(UsageProviderError("Pi offline"))])
        let clock = TestClock(now)
        let provider = CodexUsageProvider(readLimits: { a }, transcripts: CodexTranscriptStore(roots: []),
            history: QuotaHistoryStore(), clock: { clock.now }, readPiLimits: { try await steps.next() })
        await provider.refreshAccountUsage(historyHours: 24)
        clock.advance(61)
        await provider.refreshAccountUsage(historyHours: 24)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        let native = try XCTUnwrap(report.discoveredAgents.first { $0.account == a.providerAccount(home: "") })
        let pi = try XCTUnwrap(report.discoveredAgents.first { $0.account == b.providerAccount(home: "pi") })
        XCTAssertNil(report.quotaNotice(for: native))
        XCTAssertEqual(report.quotaNotice(for: pi), "Pi offline")
        XCTAssertEqual(report.snapshot(for: pi.id)?.updatedAt, now, "failure never renews the old reading")
        XCTAssertEqual(report.accounts?["Codex"]?.count, 2)
    }

    func testSameAccountResetProducesOneEventPerWindowAndOneHistorySample() async throws {
        let a = try PiCodexClient.parse(Data(Self.payload.utf8), expectedAccount: "workspace")
        let reset = try PiCodexClient.parse(Data(Self.payload.replacingOccurrences(of: "used_percent\":20", with: "used_percent\":0")
            .replacingOccurrences(of: "used_percent\":30", with: "used_percent\":0").utf8), expectedAccount: "workspace")
        let native = Steps([.success(a), .success(reset)]), pi = Steps([.success(a), .success(reset)])
        let clock = TestClock(now), history = QuotaHistoryStore()
        let provider = CodexUsageProvider(readLimits: { try await native.next()! }, transcripts: CodexTranscriptStore(roots: []),
            history: history, clock: { clock.now }, readPiLimits: { try await pi.next() })
        var tracker = QuotaAlertTracker()
        await provider.refreshAccountUsage(historyHours: 24)
        let first = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertTrue(tracker.update(report: first, agents: first.discoveredAgents, now: clock.now).alerts.isEmpty)
        clock.advance(61)
        await provider.refreshAccountUsage(historyHours: 24)
        let second = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(tracker.update(report: second, agents: second.discoveredAgents, now: clock.now).alerts.count, 2)
        let count = await history.count
        XCTAssertEqual(count, 6, "three windows sampled twice, irrespective of client count")
    }

    private actor Steps {
        var values: [Result<CodexRateLimits, UsageProviderError>]
        init(_ values: [Result<CodexRateLimits, UsageProviderError>]) { self.values = values }
        func next() throws -> CodexRateLimits? { try values.removeFirst().get() }
    }
    private final class TestClock: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Date
        init(_ value: Date) { self.value = value }
        var now: Date { lock.withLock { value } }
        func advance(_ seconds: TimeInterval) { lock.withLock { value.addTimeInterval(seconds) } }
    }
}
