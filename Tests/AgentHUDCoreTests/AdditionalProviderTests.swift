import AgentHUDSupport
import Foundation
import SQLite3
import XCTest
@testable import AgentHUDCore

final class AdditionalProviderTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1788800000)
    private func json(_ text: String) throws -> ProviderJSON { try .read(Data(text.utf8)) }
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }
    private func file(_ name: String, _ text: String) throws -> URL {
        let url = try directory().appendingPathComponent(name)
        try text.write(to: url, atomically: true, encoding: .utf8)
        return url
    }
    private func jwt(exp: Double = 2000000000, user: String = "account-a") throws -> String {
        let payload = try JSONSerialization.data(withJSONObject: ["sub": "auth0|" + user, "exp": exp])
        return "header." + payload.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "") + ".signature"
    }
    private func database(_ url: URL, _ body: (OpaquePointer) throws -> Void) throws {
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &db), SQLITE_OK)
        let handle = try XCTUnwrap(db)
        defer { sqlite3_close(handle) }
        try body(handle)
    }
    private func sql(_ db: OpaquePointer, _ query: String) throws {
        guard sqlite3_exec(db, query, nil, nil, nil) == SQLITE_OK else { throw ProviderFailure.local }
    }
    private func authDB() throws -> URL {
        let url = try directory().appendingPathComponent("state.vscdb")
        let bytes = try jwt().data(using: .utf16LittleEndian)!.map { String(format: "%02x", $0) }.joined()
        try database(url) { db in
            try sql(db, "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value BLOB)")
            try sql(db, "INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', X'\(bytes)')")
        }
        return url
    }

    func testAntigravityQuotaKeepsZeroAndOmitsUnknownOrDisabledBuckets() throws {
        let quota = try AntigravityClient.summary(json(#"{"groups":[{"displayName":"Premium","buckets":[{"bucketId":"weekly","remaining":{"case":"remainingFraction","value":0}},{"bucketId":"missing","remaining":{}},{"bucketId":"disabled","disabled":true,"remainingFraction":1}]}]}"#))
        XCTAssertEqual(quota.windows.count, 1)
        XCTAssertEqual(quota.windows[0].remaining, 0)
        XCTAssertNil(quota.windows[0].reset, "A cadence label cannot establish a reset instant")
        XCTAssertEqual(quota.windows[0].duration, 604800)
    }

    func testAntigravityOnlyDiscoversItsOwnServersAndQuotedFlags() {
        let rows = AntigravityClient.candidates("""
        11 /Applications/Antigravity.app/Contents/Resources/language_server --csrf_token 'fixture csrf' --extension_server_port=42111
        12 /Applications/Other.app/language_server --csrf_token other
        13 /usr/local/bin/agy serve
        """)
        XCTAssertEqual(rows.map(\.pid), [11, 13])
        XCTAssertEqual(rows.first?.token, "fixture csrf")
        XCTAssertEqual(rows.first?.extensionPort, 42111)
        XCTAssertEqual(AntigravityClient.ports("n127.0.0.1:42111\nn*:42111\nn[::1]:42112\n"), [42111, 42112])
    }

    func testGrokQuotaDistinguishesSubscriptionAndExtraBudget() throws {
        let quota = try GrokClient.parse(json(#"{"config":{"creditUsagePercent":12.5,"currentPeriod":{"start":"2026-09-01T00:00:00Z","end":"2026-09-08T00:00:00Z","type":"USAGE_PERIOD_TYPE_WEEKLY"},"onDemandCap":{"val":20},"onDemandUsed":{"val":3}}}"#))
        XCTAssertEqual(quota.windows.map(\.remaining), [87.5, 85])
        XCTAssertEqual(quota.windows[0].duration, 604800)
        XCTAssertTrue(try GrokClient.parse(json(#"{"config":{}}"#)).windows.isEmpty)
    }

    func testGrokAuthUsesOnlySupportedNonexpiredIssuer() throws {
        let credential = try GrokClient.credential(json(#"{"https://auth.x.ai::cli":{"key":"fixture-current","expires_at":"2030-01-01T00:00:00Z"},"https://accounts.x.ai/sign-in":{"key":"fixture-legacy","expires_at":"2030-01-01T00:00:00Z"},"https://example.com":{"key":"other","expires_at":"2030-01-01T00:00:00Z"}}"#), now: now)
        XCTAssertEqual(credential["key"].stringValue, "fixture-current")
        XCTAssertThrowsError(try GrokClient.credential(json(#"{"https://auth.x.ai::cli":{"key":"expired","expires_at":"2020-01-01T00:00:00Z"}}"#), now: now))
    }

    func testCursorFractionalPercentIsAlreadyAPercentage() throws {
        let quota = try CursorClient.parseQuota(json(#"{"membershipType":"pro","individualUsage":{"plan":{"totalPercentUsed":0.36,"autoPercentUsed":0.1},"onDemand":{"used":5,"limit":10}}}"#))
        XCTAssertEqual(quota.windows.map(\.remaining), [99.64, 99.9, 50])
        XCTAssertNil(quota.windows.first?.reset)
        XCTAssertTrue(try CursorClient.parseQuota(json(#"{"membershipType":"free"}"#)).windows.isEmpty)
    }

    func testCursorReadsLiveWALAndUTF16WithoutWritingCredentials() async throws {
        let url = try authDB(), token = try jwt(user: "changed")
        let client = CursorClient(database: url)
        try database(url) { db in
            try sql(db, "PRAGMA journal_mode=WAL")
            try sql(db, "UPDATE ItemTable SET value='\(token)' WHERE key='cursorAuth/accessToken'")
            let reader = try ReadOnlySQLite(url)
            var value: String?
            try reader.rows("SELECT value FROM ItemTable") { value = ReadOnlySQLite.text($0, 0) }
            XCTAssertEqual(value, token)
        }
        let actual = try await client.session(now: now)
        XCTAssertTrue(actual.cookie.hasPrefix("WorkosCursorSessionToken=changed%3A%3A"))
        let utf16 = CursorClient(database: try authDB())
        let original = try await utf16.session(now: now)
        XCTAssertEqual(original.account, RecordCoding.hash(["account-a"]))
        XCTAssertThrowsError(try CursorClient.session(token: jwt(exp: 1), now: now))
    }

    func testCursorPaginationPreservesRealDuplicatesAndRemovesOnlyProvenBoundaryOverlap() throws {
        let a: ProviderJSON = .string("a"), b: ProviderJSON = .string("b"), c: ProviderJSON = .string("c")
        XCTAssertEqual(try CursorClient.reconcile(pages: [[a, a], [b]], expected: 3), [a, a, b])
        XCTAssertEqual(try CursorClient.reconcile(pages: [[a, b], [b, c]], expected: 3), [a, b, c])
        XCTAssertThrowsError(try CursorClient.reconcile(pages: [[a, b], [c]], expected: 4))
        XCTAssertThrowsError(try CursorClient.reconcile(pages: [[a, b], [c]], expected: 2))
    }

    func testCursorEventsHaveAccountIdentityAndDisjointTokenCounts() throws {
        let row = try json(#"{"timestamp":"1788800000000","conversationId":"conversation","model":"model","tokenUsage":{"inputTokens":10,"outputTokens":20,"cacheReadTokens":30,"cacheWriteTokens":5}}"#)
        let a = try CursorClient.parseEvents([row, row], account: "a")
        let b = try CursorClient.parseEvents([row], account: "b")
        XCTAssertTrue(a.sessions[0].accountWide)
        XCTAssertNotEqual(a.sessions[0].id, b.sessions[0].id)
        XCTAssertEqual(Set(a.sessions[0].events.map(\.id)).count, 2)
        XCTAssertEqual(a.sessions[0].events[0].input, 15)
        XCTAssertEqual(a.sessions[0].events[0].cacheRead, 30)
        XCTAssertThrowsError(try CursorClient.parseEvents([json(#"{"timestamp":1788800000000,"model":"x","tokenUsage":{"inputTokens":1,"outputTokens":2,"cacheReadTokens":-1}}"#)], account: "a"))
    }

    private actor Requests {
        var values: [URLRequest] = []
        func record(_ value: URLRequest) { values.append(value) }
    }
    func testCursorCachesInitialDashboardFailure() async throws {
        let requests = Requests()
        let client = CursorClient(database: try authDB(), http: ProviderHTTP(send: { request in
            await requests.record(request)
            throw ProviderHTTPError(status: 503)
        }))
        let first = await client.sessions(since: now.addingTimeInterval(-86400))
        let second = await client.sessions(since: now.addingTimeInterval(-86400))
        let count = await requests.values.count
        XCTAssertEqual(count, 1)
        XCTAssertNotNil(first.notice)
        XCTAssertEqual(first.notice, second.notice)
    }

    func testGrokUnifiedDeduplicatesAndDoesNotAddReasoningTwice() throws {
        let usage = #"{"ts":"2026-09-07T16:53:20Z","sid":"s","pid":1,"event_id":"e","msg":"shell.turn.inference_done","ctx":{"prompt_tokens":100,"completion_tokens":20,"cached_prompt_tokens":60,"reasoning_tokens":10}}"#
        let url = try file("unified.jsonl", #"{"sid":"s","pid":1,"msg":"model changed","ctx":{"model":"grok-test"}}"# + "\n" + usage + "\n" + usage + "\n")
        let events = try GrokSessions.read(url).sessions[0].events
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events[0].input, 40)
        XCTAssertEqual(events[0].output, 20)
        XCTAssertEqual(events[0].cacheRead, 60)
        XCTAssertEqual(events[0].model, "grok-test")
    }

    func testGrokLegacyContextCountersDoNotBecomeConsumption() throws {
        let root = try directory(), url = root.appendingPathComponent("updates.jsonl"), id = root.lastPathComponent
        let lines = [
            "{\"method\":\"session/update\",\"params\":{\"sessionId\":\"\(id)\",\"update\":{\"sessionUpdate\":\"user_message_chunk\"},\"_meta\":{\"agentTimestampMs\":1788800000000}}}",
            "{\"method\":\"session/update\",\"params\":{\"sessionId\":\"\(id)\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\"},\"_meta\":{\"agentTimestampMs\":1788800001000,\"totalTokens\":1234}}}"
        ]
        try (lines.joined(separator: "\n") + "\n").write(to: url, atomically: true, encoding: .utf8)
        let result = try GrokSessions.read(url)
        XCTAssertTrue(result.sessions[0].events.isEmpty)
        XCTAssertNotNil(result.notice)
        XCTAssertEqual(result.sessions[0].turns.first?.state, .running)
    }

    private func varint(_ value: UInt64) -> [UInt8] {
        var value = value, result: [UInt8] = []
        repeat { var byte = UInt8(value & 127); value >>= 7; if value > 0 { byte |= 128 }; result.append(byte) } while value > 0
        return result
    }
    private func number(_ field: UInt64, _ value: UInt64) -> [UInt8] { varint(field << 3) + varint(value) }
    private func message(_ field: UInt64, _ bytes: [UInt8]) -> [UInt8] { varint((field << 3) | 2) + varint(UInt64(bytes.count)) + bytes }
    private func generation(timestamp: Bool = true) -> [UInt8] {
        let usage = number(1, 10) + number(2, 20) + number(5, 40) + number(9, 5) + number(10, 3) + message(11, Array("response".utf8))
        let stamp = timestamp ? message(9, message(4, number(1, 1788800000))) : message(9, message(10, [1, 2, 3, 4, 5, 6, 7, 8]))
        return message(1, message(4, usage) + message(19, Array("gemini-test".utf8)) + stamp) + message(4, Array("step".utf8))
    }

    func testAntigravitySQLiteUsesRecordedTimeAndSeparateThinkingTokens() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            let hex = generation().map { String(format: "%02x", $0) }.joined()
            try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)'), (1, X'\(hex)')")
        }
        let events = try AntigravitySessions.read(url).sessions[0].events
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events[0].timestamp, now)
        XCTAssertEqual(events[0].input, 30)
        XCTAssertEqual(events[0].output, 8)
        XCTAssertEqual(events[0].cacheRead, 40)
    }

    func testAntigravityRejectsOpaqueTimeAndAmbiguousStepJoin() throws {
        let turn = try XCTUnwrap(AntigravityProtoReader.parseTurn(generation(timestamp: false)))
        XCTAssertNil(turn.timestampMs)
        let step = AntigravityProtoReader.StepMetadata(stepUUID: "step", botID: nil, timestampMs: 1788800000000)
        XCTAssertEqual(AntigravitySessions.matchedTimestamp(turn, generations: [turn], steps: [step]), 1788800000000)
        XCTAssertNil(AntigravitySessions.matchedTimestamp(turn, generations: [turn, turn], steps: [step]))
        XCTAssertNil(AntigravitySessions.matchedTimestamp(turn, generations: [turn], steps: [step, step]))
        XCTAssertNil(try AntigravityProtoReader.parseTurn([0x0a, 0xff]))
    }



    func testUsageIdentityKeepsCorrectionsAndDistinctEqualRequests() {
        func event(_ id: String?, output: Int = 20, cache: Int = 0) -> UsageEvent {
            .init(timestamp: now, agentId: "cursor-model:x", tokensIn: 10, tokensOut: output, cacheReadTokens: cache, eventID: id)
        }
        let merged = UsageAggregation.usageUnion([[event("a", output: 30), event("b")], [event("a"), event("b")]])
        XCTAssertEqual(merged.count, 2)
        XCTAssertEqual(merged.first { $0.eventID == "a" }?.tokensOut, 30)
        XCTAssertEqual(UsageAggregation.usageUnion([[event("a"), event("b")], [event("a"), event("b")]]).count, 2)
        let legacy = UsageAggregation.usageUnion([[event("a")], [event(nil, cache: 5)]])
        XCTAssertEqual(legacy.count, 1)
        XCTAssertEqual(legacy[0].eventID, "a")
        XCTAssertEqual(legacy[0].cacheReadTokens, 5)
    }



    func testGrokProcessModelIsNotInheritedAcrossPIDReuse() throws {
        let url = try file("unified.jsonl", """
        {"pid":1,"msg":"AuthManager::new"}
        {"pid":1,"msg":"model catalog: notifying clients","ctx":{"current_model_id":"first-model"}}
        {"pid":1,"sid":"s","ts":1788800000000,"msg":"shell.turn.inference_done","ctx":{"prompt_tokens":1,"completion_tokens":2}}
        {"pid":1,"msg":"AuthManager::new"}
        {"pid":1,"sid":"s","ts":1788800001000,"msg":"shell.turn.inference_done","ctx":{"prompt_tokens":3,"completion_tokens":4}}

        """)
        let result = try GrokSessions.read(url)
        XCTAssertEqual(result.sessions[0].events.map(\.model), ["first-model", "Unknown"])
    }

    func testCompleteLastLineIsReadAndTornTailIsRetried() throws {
        let url = try file("session.jsonl", "{\"kind\":0}\n{\"kind\":1}")
        var kinds: [Int] = []
        try ProviderFiles.lines(url) { value, _ in kinds.append(value["kind"].countValue!) }
        XCTAssertEqual(kinds, [0, 1])
        try "{\"kind\":0}\n{\"ki".write(to: url, atomically: true, encoding: .utf8)
        kinds = []
        try ProviderFiles.lines(url) { value, _ in kinds.append(value["kind"].countValue!) }
        XCTAssertEqual(kinds, [0])
    }

    @MainActor
    func testAntigravityPlaceholderMigratesWithoutLosingPreferences() {
        let name = "AdditionalProviderTests.\(UUID().uuidString)", defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        let placeholder = AgentDescriptor(id: "antigravity", vendor: "Antigravity", model: "Agent", source: L10n.sourceNotConnected,
            enabled: true, connected: false)
        let settings = SettingsStore(defaults: defaults, defaultAgents: [placeholder])
        let windows = ["one", "two"].map { AgentDescriptor(id: "antigravity:\($0)", vendor: "Antigravity", model: $0,
            source: L10n.sourceAdditionalUsage, enabled: false) }
        settings.mergeDiscovered(windows); settings.mergeDiscovered(windows)
        XCTAssertEqual(settings.agents.count, 2)
        XCTAssertTrue(settings.agents.allSatisfy { $0.enabled && $0.connected })
    }

    func testAdditionalProviderCachesQuotaAndStillReportsLocalUsageWhenSignedOut() async throws {
        let now = now, history = QuotaHistoryStore(fileURL: nil)
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { ProviderQuota(windows: [.init(id: "grok", label: "Credits", remaining: 80)]) },
            readSessions: { _ in ProviderSessions(sessions: [.init(id: "s", title: "Fixture", client: "Grok CLI", events: [.init(id: "e", model: "grok-test", timestamp: now, input: 10, output: 20)])]) },
            history: history, clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let count = await history.count
        XCTAssertEqual(count, 1)
        XCTAssertEqual(report.consumption.first?.eventID, "grok:e")
        XCTAssertEqual(report.consumers.first?.vendor, "Grok")
        XCTAssertEqual(report.consumerIdsByQuota["grok"], ["grok-model:grok-test"])
        let unavailable = AdditionalUsageProvider(source: .grok, readQuota: { throw ProviderFailure.login("Grok") },
            readSessions: { _ in ProviderSessions(sessions: [.init(id: "s", title: "Fixture", client: "Grok CLI", events: [.init(id: "e", model: "x", timestamp: now, input: 1, output: 2)])]) },
            history: history, clock: { now })
        let local = try await unavailable.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(local.consumption.count, 1)
        XCTAssertTrue(local.snapshots.isEmpty)
        XCTAssertNotNil(local.sourceNotices["Grok"])
    }

    /// Explicit local smoke probe. Prints only counts and sanitized provider errors; never credential values or session content.
    func testInstalledSourcesReadOnlyProbe() async throws {
        guard ProcessInfo.processInfo.environment["AGENT_HUD_PROBE_ADDITIONAL"] == "1" else { throw XCTSkip("Set AGENT_HUD_PROBE_ADDITIONAL=1 for a read-only local probe") }
        for source in AdditionalSource.allCases {
            let report = try await AdditionalUsageProvider.standard(source, persistHistory: false).fetchAccountAndLocalUsage(agents: [], historyHours: 168)
            print("Probe \(source.vendor): quotas=\(report.snapshots.count), sessions=\(report.sessions.count), events=\(report.consumption.count), completions=\(report.completions.count), indexing=\(report.indexing != nil), notice=\(report.sourceNotices[source.vendor] ?? "none")")
        }
    }
}
