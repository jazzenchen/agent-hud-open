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
        XCTAssertEqual(quota.quotaWindowIDs, ["antigravity:weekly", "antigravity:missing"],
                       "Complete presence includes enabled buckets without values, while disabled buckets retire")
        XCTAssertEqual(quota.windows[0].remaining, 0)
        XCTAssertNil(quota.windows[0].reset, "A cadence label cannot establish a reset instant")
        XCTAssertEqual(quota.windows[0].duration, 604800)
    }

    /// The bucket names of Antigravity's usage panel, as its models page shows them; the fixture is derived from that page,
    /// not captured from an account.
    func testAntigravityReadsItsOwnBucketNamesAsTheirPeriods() throws {
        let quota = try AntigravityClient.summary(json(#"{"groups":[{"displayName":"Gemini Models","buckets":[{"bucketId":"gemini-5h","displayName":"Five Hour Limit","remainingFraction":0.5},{"bucketId":"gemini-7d","displayName":"Weekly Limit","remainingFraction":0.75}]},{"displayName":"Claude and GPT models","buckets":[{"bucketId":"third-party-5h","displayName":"FIVE HOUR LIMIT","remainingFraction":1},{"bucketId":"third_party_five_hour","remainingFraction":1}]}]}"#))
        XCTAssertEqual(quota.windows.map(\.label), ["Gemini Models · Five Hour Limit", "Gemini Models · Weekly Limit",
                                                    "Claude and GPT models · FIVE HOUR LIMIT", "Claude and GPT models · third_party_five_hour"])
        XCTAssertEqual(quota.windows.map(\.duration), [18000, 604800, 18000, 18000])
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

    func testAntigravityRefreshesSummaryWithoutReplacingItWithOlderModelQuota() async throws {
        let requests = Requests()
        let endpoint = AntigravityService.Endpoint(pid: 1,
            base: URL(string: "https://127.0.0.1:42111/exa.language_server_pb.LanguageServerService/")!, token: "fixture-csrf")
        let client = AntigravityClient(http: ProviderHTTP(send: { request in
            await requests.record(request)
            let method = request.url?.lastPathComponent
            if method == "RetrieveUserQuotaSummary" {
                let body = try ProviderJSON.read(try XCTUnwrap(request.httpBody))
                // The server otherwise returns a cached full bucket, as before the native popover requests a refresh.
                let remaining = body["forceRefresh"].boolValue == true ? 0 : 1
                return Data(#"{"response":{"groups":[{"displayName":"Claude and GPT models","buckets":[{"bucketId":"3p-5h","displayName":"Five Hour Limit Remaining","remainingFraction":\#(remaining)}]}]}}"#.utf8)
            }
            XCTAssertEqual(method, "GetUserStatus")
            return Data(#"{"userStatus":{"email":"quota@example.com","cascadeModelConfigData":{"clientModelConfigs":[{"label":"Claude Sonnet","quotaInfo":{"remainingFraction":0.75}}]}}}"#.utf8)
        }))

        let quota = try await client.quota(from: endpoint)
        XCTAssertEqual(quota.windows.map(\.id), ["antigravity:3p-5h"])
        XCTAssertEqual(quota.windows.map(\.remaining), [0], "the fresh summary wins over a stale legacy model reading")
        XCTAssertEqual(quota.label, "quota@example.com")
        let sent = await requests.values
        XCTAssertEqual(sent.map { $0.url?.lastPathComponent }, ["RetrieveUserQuotaSummary", "GetUserStatus"])
        XCTAssertEqual(sent.map(\.httpMethod), ["POST", "POST"])
        XCTAssertTrue(sent.allSatisfy { $0.value(forHTTPHeaderField: "X-Codeium-Csrf-Token") == "fixture-csrf" })
        let summary = try ProviderJSON.read(try XCTUnwrap(sent.first?.httpBody))
        XCTAssertEqual(summary["forceRefresh"].boolValue, true)
        let status = try ProviderJSON.read(try XCTUnwrap(sent.last?.httpBody))
        XCTAssertEqual(status["metadata"]["ideName"].stringValue, "antigravity")
        XCTAssertNil(status["forceRefresh"].boolValue, "only the quota summary uses the native force-refresh option")
    }

    func testAWholeFileStoreLooksOnlyAtThePathsTheWatchReports() async throws {
        let root = try directory()
        func session(_ id: String) throws -> URL {
            let folder = root.appendingPathComponent("%2Ffixture/\(id)")
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let file = folder.appendingPathComponent("updates.jsonl")
            try (#"{"method":"_x.ai/session/update","params":{"sessionId":"\#(id)","_meta":{"eventId":"done","agentTimestampMs":1788800002000},"update":{"sessionUpdate":"turn_completed","prompt_id":"p","stop_reason":"end_turn","usage":{"inputTokens":100,"outputTokens":20,"modelUsage":{"grok-test":{}}}}}}"#
                + "\n").write(to: file, atomically: true, encoding: .utf8)
            return file
        }
        _ = try session("a")
        let store = AdditionalLocalStore(source: .grok, roots: [root])
        var sessions = await store.index(since: .distantPast).sessions
        XCTAssertEqual(sessions.count, 1)
        // The collector's watch runs and saw nothing here.
        await store.fileChanges([])
        let added = try session("b")
        sessions = await store.index(since: .distantPast).sessions
        XCTAssertEqual(sessions.count, 1, "a read the watch did not ask for does not list the tree again")
        await store.fileChanges([added.path])
        sessions = await store.index(since: .distantPast).sessions
        XCTAssertEqual(sessions.count, 2)
    }

    func testGrokQuotaDistinguishesSubscriptionAndExtraBudget() throws {
        let quota = try GrokClient.parse(json(#"{"config":{"creditUsagePercent":12.5,"currentPeriod":{"start":"2026-09-01T00:00:00Z","end":"2026-09-08T00:00:00Z","type":"USAGE_PERIOD_TYPE_WEEKLY"},"onDemandCap":{"val":20},"onDemandUsed":{"val":3}}}"#))
        XCTAssertEqual(quota.windows.map(\.remaining), [87.5, 85])
        XCTAssertEqual(quota.windows[0].duration, 604800)
        let unknown = try GrokClient.parse(json(#"{"config":{}}"#))
        XCTAssertEqual(unknown.windows.map(\.id), ["grok"])
        XCTAssertNil(unknown.windows[0].remaining)
        let extraOnly = try GrokClient.parse(json(#"{"config":{"onDemandCap":{"val":20},"onDemandUsed":{"val":3}}}"#))
        XCTAssertEqual(extraOnly.windows.map(\.id), ["grok", "grok:extra"])
        XCTAssertNil(extraOnly.windows[0].remaining)
        XCTAssertNil(extraOnly.notice, "a subscription share the service left out is no failed read")
        XCTAssertNotNil(extraOnly.displayNotice)
    }

    /// A notice about what a quota answer left out is shown, and the windows the answer did give keep their levels.
    @MainActor
    func testWhatAQuotaAnswerLeftOutIsShownWithoutHoldingItsWindowsBack() async throws {
        let now = now
        let provider = AdditionalUsageProvider(source: .grok, readQuota: {
            try GrokClient.parse(ProviderJSON.read(Data(#"{"config":{"onDemandCap":{"val":20},"onDemandUsed":{"val":15}}}"#.utf8)))
        }, readSessions: { _ in ProviderSessions() }, history: QuotaHistoryStore(), clock: { now })
        await provider.refreshAccountUsage(historyHours: 48)
        let report = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(report.sourceNotices["Grok"], "Grok is connected, but used credits were not reported")
        XCTAssertEqual(report.readingIssues, [:])
        XCTAssertEqual(report.quotaNotices, [:])
        let suite = "AdditionalProviderTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: report.discoveredAgents))
        store.replace(report: report)
        store.now = now
        XCTAssertEqual(store.rows.last?.level, .warning, "the extra budget read keeps its level")
        XCTAssertNil(store.rows.first?.remainingPct, "the omitted subscription percentage stays unknown")
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
        let free = try CursorClient.parseQuota(json(#"{"membershipType":"free"}"#))
        XCTAssertTrue(free.windows.isEmpty)
        XCTAssertNil(free.notice, "a plan that reports no percentage is no failed read")
        XCTAssertNotNil(free.displayNotice)
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

    func testCursorNamesConversationsFromTheIDEAndTheAgentCLI() async throws {
        let url = try authDB(), agent = try directory()
        try database(url) { db in
            try sql(db, "CREATE TABLE composerHeaders (composerId TEXT PRIMARY KEY, value TEXT)")
            try sql(db, #"INSERT INTO composerHeaders VALUES ('ide', '{"name":"Codebase tour"}'), ('empty', '{}')"#)
        }
        for (folder, id, title) in [("acp-sessions", "acp", "Feishu round one"), ("chats/0cc175b9", "cli", "New Agent")] {
            let meta = agent.appendingPathComponent("\(folder)/\(id)/meta.json")
            try FileManager.default.createDirectory(at: meta.deletingLastPathComponent(), withIntermediateDirectories: true)
            try #"{"schemaVersion":1,"title":"\#(title)"}"#.write(to: meta, atomically: true, encoding: .utf8)
        }
        let rows = try ["ide", "empty", "acp", "cli", "elsewhere"].map {
            try json(#"{"timestamp":"1788800000000","conversationId":"\#($0)","model":"m","tokenUsage":{"inputTokens":1,"outputTokens":1}}"#)
        }
        let parsed = try CursorClient.parseEvents(rows, account: "a")
        let named = await CursorClient(database: url, agentFolder: agent).named(parsed, account: "a")
        XCTAssertEqual(named.sessions.map(\.title), ["Feishu round one", "Cursor · cli", "Cursor · elsewher", "Cursor · empty", "Codebase tour"],
                       "a chat still called New Agent, or run elsewhere, keeps its placeholder")
    }

    private actor Requests {
        var values: [URLRequest] = []
        func record(_ value: URLRequest) { values.append(value) }
    }
    /// Every account step asks the dashboard, since the collector decides how often; a failure keeps its notice.
    func testCursorKeepsADashboardFailureNotice() async throws {
        let requests = Requests()
        let client = CursorClient(database: try authDB(), http: ProviderHTTP(send: { request in
            await requests.record(request)
            throw ProviderHTTPError(status: 503)
        }))
        let first = await client.sessions(since: now.addingTimeInterval(-86400))
        let second = await client.sessions(since: now.addingTimeInterval(-86400))
        let count = await requests.values.count
        XCTAssertEqual(count, 2)
        XCTAssertNotNil(first.notice)
        XCTAssertEqual(first.notice, second.notice)
    }

    func testCursorUsageKeepsItsAccountThroughAFailedQuotaReading() async throws {
        final class State: @unchecked Sendable { var reads = 0; var now = Date(timeIntervalSince1970: 1_788_800_000) }
        let state = State(), ledger = UsageLedger.inMemory(), start = state.now
        let account = ProviderAccount(provider: "Cursor", user: "user", workspace: "", evidence: .account)
        let conversation = ProviderSession(id: "cursor-account:a:c", title: "Chat", client: "Cursor",
            events: [ProviderEvent(id: "e", model: "cursor-test", timestamp: start.addingTimeInterval(-60), input: 10, output: 1)], accountWide: true)
        let provider = AdditionalUsageProvider(source: .cursor, readQuota: {
            state.reads += 1
            guard state.reads == 1 else { throw ProviderHTTPError(status: 503) }
            return ProviderQuota(windows: [.init(id: "cursor", label: "Included usage", remaining: 60, shortLabel: "Included"),
                                           .init(id: "cursor:team", label: "Pooled usage", remaining: 60)], account: account)
        }, readSessions: { _ in ProviderSessions(sessions: [conversation]) }, history: QuotaHistoryStore(), clock: { state.now }, ledger: ledger)
        func accounts() async throws -> Set<String> {
            Set(try await ledger.buckets(since: .distantPast, source: AdditionalSource.cursor.rawValue).compactMap(\.account))
        }
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        XCTAssertEqual(report.discoveredAgents.map(\.shortName), ["Included", "Pooled usage"], "a window without a short name shows its full name")
        var recorded = try await accounts()
        XCTAssertEqual(recorded, [account.id])
        state.now = start.addingTimeInterval(UsageRefresh.accountRequestSpacing + 1)
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        recorded = try await accounts()
        XCTAssertEqual(recorded, [account.id], "a quota request that failed does not move the account's usage")
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

    func testGrokTakesTheTitleItKeepsCurrent() throws {
        let folder = try directory().appendingPathComponent("%2FUsers%2Fme%2Frepo/s1")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent("updates.jsonl")
        try "".write(to: url, atomically: true, encoding: .utf8)
        XCTAssertEqual(try GrokSessions.read(url).sessions.first?.title, "repo", "the folder until Grok names the session")
        try #"{"generated_title":"Weekly quota check","session_summary":"Weekly quota check","title_is_manual":false}"#
            .write(to: folder.appendingPathComponent("summary.json"), atomically: true, encoding: .utf8)
        XCTAssertEqual(try GrokSessions.read(url).sessions.first?.title, "Weekly quota check")
    }

    private func varint(_ value: UInt64) -> [UInt8] {
        var value = value, result: [UInt8] = []
        repeat { var byte = UInt8(value & 127); value >>= 7; if value > 0 { byte |= 128 }; result.append(byte) } while value > 0
        return result
    }
    private func number(_ field: UInt64, _ value: UInt64) -> [UInt8] { varint(field << 3) + varint(value) }
    private func message(_ field: UInt64, _ bytes: [UInt8]) -> [UInt8] { varint((field << 3) | 2) + varint(UInt64(bytes.count)) + bytes }
    private func generation(timestamp: Bool = true, usage recordedUsage: [UInt8]? = nil,
                            model: String? = "gemini-test", executionID: String = "step") -> [UInt8] {
        // CortexStepGeneratorMetadata.chat_model -> ChatModelMetadata.usage -> ModelUsageStats.
        var defaultUsage = number(1, 1405)
        defaultUsage += number(2, 20)
        defaultUsage += number(3, 8)
        defaultUsage += number(5, 40)
        defaultUsage += number(9, 5)
        defaultUsage += number(10, 3)
        defaultUsage += message(11, Array("response".utf8))
        let usage = recordedUsage ?? defaultUsage
        let stamp = timestamp ? message(9, message(4, number(1, 1788800000))) : message(9, message(10, [1, 2, 3, 4, 5, 6, 7, 8]))
        let name = model.map { message(19, Array($0.utf8)) } ?? []
        return message(1, number(3, 1405) + message(4, usage) + name + stamp) + message(4, Array(executionID.utf8))
    }

    func testAntigravitySQLiteUsesRecordedTokensAndIncludesThinkingOnlyOnce() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            let hex = generation().map { String(format: "%02x", $0) }.joined()
            try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)'), (1, X'\(hex)')")
        }
        let events = try AntigravitySessions.read(url).sessions[0].events
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events[0].timestamp, now)
        XCTAssertEqual(events[0].input, 20, "the model enum 1405 is not input usage")
        XCTAssertEqual(events[0].output, 8)
        XCTAssertEqual(events[0].cacheRead, 40)
        XCTAssertEqual(events[0].reasoning, 5, "thinking is a subset of total output")
        XCTAssertEqual(events[0].model, "gemini-test")
    }

    func testAntigravityCacheWritesAreIncludedInInputAndRetainedAsDetail() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        let usage = number(1, 1405) + number(2, 20) + number(3, 8) + number(4, 7) + number(5, 40)
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            let hex = generation(usage: usage, model: nil).map { String(format: "%02x", $0) }.joined()
            try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)')")
        }
        let event = try XCTUnwrap(AntigravitySessions.read(url).sessions[0].events.first)
        XCTAssertEqual(event.input, 27)
        XCTAssertEqual(event.cacheWrite, 7)
        XCTAssertEqual(event.cacheRead, 40, "cached reads are separate from input")
        XCTAssertEqual(event.output, 8, "total output remains authoritative when component counters are omitted")
        XCTAssertEqual(event.reasoning, 0)
        XCTAssertEqual(event.model, "Antigravity model 1405", "an unnamed model enum remains an identity")
    }

    func testAntigravityModelOnlyFailedGenerationIsNotMissingUsage() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        let completed = generation()
        // The live failed generations carry the model enum, retries and error, but no recorded tokens or date.
        let failed = generation(timestamp: false, usage: number(1, 1405), model: nil)
            + message(5, Array("fixture generation failure".utf8))
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            for (index, bytes) in [completed, failed].enumerated() {
                let hex = bytes.map { String(format: "%02x", $0) }.joined()
                try sql(db, "INSERT INTO gen_metadata VALUES (\(index), X'\(hex)')")
            }
        }
        let result = try AntigravitySessions.read(url)
        XCTAssertNil(result.notice)
        XCTAssertEqual(result.sessions[0].events.count, 1)
        XCTAssertEqual(result.sessions[0].events[0].input, 20)
    }

    func testAntigravityRecordedUsageWithoutVerifiableTimeStillWarns() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        let usages = [number(2, 20), number(3, 8), number(4, 7), number(5, 40)]
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            for (index, usage) in usages.enumerated() {
                let bytes = generation(timestamp: false, usage: number(1, 1405) + usage,
                                       executionID: "execution-\(index)")
                let hex = bytes.map { String(format: "%02x", $0) }.joined()
                try sql(db, "INSERT INTO gen_metadata VALUES (\(index), X'\(hex)')")
            }
        }
        let result = try AntigravitySessions.read(url)
        XCTAssertNotNil(result.notice)
        XCTAssertTrue(result.sessions[0].events.isEmpty, "real token usage is never assigned an inferred date")
    }

    func testAntigravityMatchesUsageMessageToItsExactStepWithinAnExecution() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        let usage = number(1, 1405) + number(2, 20) + number(3, 8) + message(7, Array("message-1".utf8))
        let bytes = generation(timestamp: false, usage: usage)
        let metadata = message(1, number(1, 1788800000))
            + message(9, message(7, Array("message-1".utf8))) + message(12, Array("step".utf8))
        let other = message(1, number(1, 1788800100))
            + message(9, message(7, Array("message-2".utf8))) + message(12, Array("step".utf8))
        try database(url) { db in
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            try sql(db, "CREATE TABLE steps (idx INTEGER PRIMARY KEY, metadata BLOB)")
            let hex = bytes.map { String(format: "%02x", $0) }.joined()
            try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)')")
            for (index, step) in [metadata, other].enumerated() {
                let hex = step.map { String(format: "%02x", $0) }.joined()
                try sql(db, "INSERT INTO steps VALUES (\(index), X'\(hex)')")
            }
        }
        let result = try AntigravitySessions.read(url)
        XCTAssertNil(result.notice)
        XCTAssertEqual(result.sessions[0].events.count, 1)
        XCTAssertEqual(result.sessions[0].events[0].timestamp, now)
    }

    func testAntigravityDirectoryScanExcludesSummariesAndKeepsConversationLayouts() async throws {
        let home = try directory(), base = home.appendingPathComponent(".gemini")
        let conversations = ["antigravity-cli/conversations/cli.db", "antigravity/conversations/desktop.db", "antigravity/legacy.db"]
            .map { base.appendingPathComponent($0) }
        let hex = generation().map { String(format: "%02x", $0) }.joined()
        for url in conversations {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try database(url) { db in
                try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
                try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)')")
            }
        }
        let summaries = base.appendingPathComponent("antigravity/conversation_summaries.db")
        try database(summaries) { db in try sql(db, "CREATE TABLE conversation_summaries (id TEXT PRIMARY KEY)") }

        let store = AdditionalLocalStore(source: .antigravity, roots: AntigravitySessions.roots(home: home, environment: [:]))
        let result = await store.index(since: .distantPast)
        XCTAssertNil(result.notice)
        XCTAssertEqual(Set(result.sessions.map(\.id)), ["antigravity:cli", "antigravity:desktop", "antigravity:legacy"])
        XCTAssertEqual(result.sessions.map { $0.events.count }, [1, 1, 1])
        let filenames = Set(conversations.map(\.lastPathComponent))
        XCTAssertEqual(result.files.map { Set($0.paths.map { URL(fileURLWithPath: $0).lastPathComponent }) }, filenames)

        await store.fileChanges([summaries.path])
        let changed = await store.index(since: .distantPast)
        XCTAssertNil(changed.notice, "a summary database change is also ignored by the collector's watch")
        XCTAssertEqual(changed.files.map { Set($0.paths.map { URL(fileURLWithPath: $0).lastPathComponent }) }, filenames)
    }

    func testAntigravityTakesTheTitleFromItsAnnotations() throws {
        let root = try directory(), url = root.appendingPathComponent("conversations/0e9830ef-dce6.db")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try database(url) { db in try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)") }
        XCTAssertEqual(try AntigravitySessions.read(url).sessions.first?.title, "Antigravity · 0e9830ef", "a headless run has no title")
        let annotations = AntigravitySessions.annotations(url)
        XCTAssertEqual(annotations.path, root.appendingPathComponent("annotations/0e9830ef-dce6.pbtxt").path)
        try FileManager.default.createDirectory(at: annotations.deletingLastPathComponent(), withIntermediateDirectories: true)
        try #"pinned:true title:"Fix \"quota\" \344\275\240 \u4f60\x21""#.write(to: annotations, atomically: true, encoding: .utf8)
        XCTAssertEqual(try AntigravitySessions.read(url).sessions.first?.title, "Fix \"quota\" 你 你!")
        XCTAssertTrue(AntigravitySessions.related(url).contains(annotations), "a rename rereads the conversation")
        XCTAssertNil(AntigravitySessions.textField("title", in: Array(#"subtitle:"x""#.utf8)))
    }

    func testAntigravityReadsWALDatabaseWhoseWriterRemovedItsFiles() throws {
        let url = try directory().appendingPathComponent("conversation.db")
        let hex = generation().map { String(format: "%02x", $0) }.joined()
        try database(url) { db in
            try sql(db, "PRAGMA journal_mode=WAL")
            try sql(db, "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB)")
            try sql(db, "INSERT INTO gen_metadata VALUES (0, X'\(hex)')")
        }
        // agy removes the -wal and -shm files when it exits; the system SQLite keeps them on close.
        let files = ["-wal", "-shm"].map { url.path + $0 }
        for file in files { try? FileManager.default.removeItem(atPath: file) }
        XCTAssertEqual(try AntigravitySessions.read(url).sessions[0].events.count, 1)
        XCTAssertFalse(files.contains { FileManager.default.fileExists(atPath: $0) }, "reading creates no files beside the database")
        let reader = try ReadOnlySQLite(url)
        try database(url) { db in try sql(db, "DELETE FROM gen_metadata") }
        XCTAssertThrowsError(try reader.rows("SELECT idx FROM gen_metadata") { _ in }, "a write during an immutable read fails it")
    }

    func testAntigravityRejectsOpaqueTimeAndAmbiguousStepJoin() throws {
        let turn = try XCTUnwrap(AntigravityProtoReader.parseTurn(generation(timestamp: false)))
        XCTAssertNil(turn.timestampMs)
        let step = AntigravityProtoReader.StepMetadata(executionID: "step", messageID: nil, timestampMs: 1788800000000)
        XCTAssertEqual(AntigravitySessions.matchedTimestamp(turn, generations: [turn], steps: [step]), 1788800000000)
        XCTAssertNil(AntigravitySessions.matchedTimestamp(turn, generations: [turn, turn], steps: [step]))
        XCTAssertNil(AntigravitySessions.matchedTimestamp(turn, generations: [turn], steps: [step, step]))
        XCTAssertNil(try AntigravityProtoReader.parseTurn([0x0a, 0xff]))
    }

    func testGrokLogPrecedenceIsResolvedBeforeRecording() {
        func event(_ id: String, priority: Int, tokens: Int) -> UsageEvent {
            .init(timestamp: now, agentId: "grok-model:x", tokensIn: tokens, tokensOut: 1, eventID: id,
                origin: .init(group: "grok:session", priority: priority))
        }
        let resolved = SessionContributions.canonical([(id: "grok:session", events: [event("old-turn-total", priority: 1, tokens: 100),
                                                                                    event("new-inference", priority: 2, tokens: 40)])])
        XCTAssertEqual(resolved["grok:session"]?.map(\.key), ["new-inference"], "only the authoritative log reaches the ledger")
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

    func testLinesWithoutAMarkerAreNumberedButNotDecoded() throws {
        let url = try file("session.jsonl", "{\"kind\":0}\nnot json\n\n{\"kind\":1,\"usage\":{}}\n{\"usage\":{},\"kind\":2}")
        var read: [Int: Int] = [:]
        try ProviderFiles.lines(url, markers: [Data(#""usage""#.utf8)]) { value, line in read[line] = value["kind"].countValue }
        XCTAssertEqual(read, [4: 1, 5: 2], "skipped and empty lines keep their numbers")
    }

    func testGrokReadsEveryModelMessageAndSkipsOtherLogLines() throws {
        let url = try file("unified.jsonl", """
        {"pid":1,"sid":"a","msg":"backend_search: model switch","ctx":{"new_model":"search-model"}}
        {"pid":1,"sid":"b","msg":"model changed","ctx":{"model":"changed-model"}}
        {"pid":1,"sid":"c","msg":"model catalog: notifying clients","ctx":{"current_model_id":"catalog-model"}}
        {"pid":1,"sid":"a","msg":"render frame","ctx":{"detail":"never decoded
        {"pid":1,"sid":"a","ts":"2026-09-07T16:53:20.100Z","msg":"shell.turn.inference_done","ctx":{"prompt_tokens":1,"completion_tokens":2}}
        {"pid":1,"sid":"b","ts":"2026-09-07T16:53:21.200Z","msg":"shell.turn.inference_done","ctx":{"prompt_tokens":3,"completion_tokens":4}}
        {"pid":1,"sid":"c","ts":"2026-09-07T16:53:22.300Z","msg":"shell.turn.inference_done","ctx":{"prompt_tokens":5,"completion_tokens":6}}

        """)
        let sessions = try GrokSessions.read(url).sessions
        XCTAssertEqual(sessions.map(\.id), ["grok:a", "grok:b", "grok:c"])
        XCTAssertEqual(sessions.flatMap(\.events).map(\.model), ["search-model", "changed-model", "catalog-model"])
    }

    func testAdditionalProviderCachesQuotaAndStillReportsLocalUsageWhenSignedOut() async throws {
        let now = now, history = QuotaHistoryStore()
        let provider = AdditionalUsageProvider(source: .grok, readQuota: { ProviderQuota(windows: [.init(id: "grok", label: "Credits", remaining: 80)]) },
            readSessions: { _ in ProviderSessions(sessions: [.init(id: "s", title: "Fixture", client: "Grok CLI", events: [.init(id: "e", model: "grok-test", timestamp: now, input: 10, output: 20)])]) },
            history: history, clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let count = await history.count
        XCTAssertEqual(count, 1)
        let recorded = await provider.usage(since: .distantPast)
        XCTAssertEqual(recorded.map(\.tokensIn), [10])
        XCTAssertEqual(recorded.map(\.agentId), ["grok-model:grok-test"])
        XCTAssertEqual(report.consumers.first?.vendor, "Grok")
        XCTAssertEqual(report.consumerIdsByQuota[ProviderAccount.unresolved(provider: "Grok", home: "").windowID("grok")], ["grok-model:grok-test"])
        let unavailable = AdditionalUsageProvider(source: .grok, readQuota: { throw ProviderFailure.login("Grok") },
            readSessions: { _ in ProviderSessions(sessions: [.init(id: "s", title: "Fixture", client: "Grok CLI", events: [.init(id: "e", model: "x", timestamp: now, input: 1, output: 2)])]) },
            history: history, clock: { now })
        let local = try await unavailable.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let localUsage = await unavailable.usage(since: .distantPast)
        XCTAssertEqual(localUsage.count, 1, "a signed-out source still records local usage")
        XCTAssertTrue(local.snapshots.isEmpty)
        XCTAssertNotNil(local.quotaNotices?["Grok"])
        XCTAssertNil(local.sourceNotices["Grok"], "quota failures are separate from local-data details")
    }

    @MainActor
    func testOnlyAFailedQuotaReadHoldsBackAlertsAndLevels() async throws {
        final class Readings: @unchecked Sendable {
            var remaining: [Double?], now: Date
            init(_ remaining: [Double?], now: Date) { self.remaining = remaining; self.now = now }
        }
        let readings = Readings([50, 5, nil], now: now), reset = now.addingTimeInterval(86400)
        let provider = RetainedUsageProvider(provider: AdditionalUsageProvider(source: .cursor, readQuota: {
            guard let remaining = readings.remaining.removeFirst() else { throw ProviderFailure.login("Cursor") }
            return ProviderQuota(windows: [.init(id: "cursor", label: "Included", remaining: remaining, reset: reset)])
        }, readSessions: { _ in ProviderSessions(notice: "Cursor usage events could not be read") }, history: QuotaHistoryStore(),
           readCompletions: { _ in throw ProviderFailure.local }, clock: { readings.now }))
        let suite = "AdditionalProviderTests.\(UUID().uuidString)", defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        var tracker = QuotaAlertTracker()
        let first = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let row = try XCTUnwrap(first.discoveredAgents.first)
        let store = UsageStore(provider: DemoUsageProvider(), settings: SettingsStore(defaults: defaults, defaultAgents: [row]))
        _ = tracker.update(report: first, agents: [row], now: readings.now)

        readings.now = now.addingTimeInterval(UsageRefresh.accountRequestSpacing)
        let low = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(low.sourceNotices["Cursor"]?.hasPrefix("Cursor usage events could not be read · "), true, "local and hook notices stay shown")
        XCTAssertEqual(tracker.update(report: low, agents: [row], now: readings.now).criticalAgentIDs, [row.id])
        store.replace(report: low)
        store.now = readings.now
        XCTAssertEqual(store.rows.map(\.level), [.critical])

        readings.now = now.addingTimeInterval(2 * UsageRefresh.accountRequestSpacing)
        let failed = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        XCTAssertEqual(failed.snapshot(for: row.id)?.remainingPct, 5, "the last reading stays shown")
        XCTAssertNotNil(failed.quotaNotice(for: row))
        XCTAssertEqual(failed.readingIssues?["Cursor"]?.kind, .readFailed)
        XCTAssertNil(low.readingIssues?["Cursor"], "a notice about the client's logs is not an issue with its reading")
        store.replace(report: failed)
        store.now = readings.now
        XCTAssertEqual(store.rows.map(\.level), [nil], "a failed read still takes the window out of the glow")
    }

    /// Explicit local smoke probe. Prints only counts and sanitized provider errors; never credential values or session content.
    @MainActor
    func testInstalledSourcesReadOnlyProbe() async throws {
        guard ProcessInfo.processInfo.environment["AGENT_HUD_PROBE_ADDITIONAL"] == "1" else { throw XCTSkip("Set AGENT_HUD_PROBE_ADDITIONAL=1 for a read-only local probe") }
        let settings = SettingsStore()
        for source in AdditionalSource.allCases {
            let provider = AdditionalUsageProvider.standard(source, settings: settings, ledger: .inMemory(), persistHistory: false)
            let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
            let buckets = await provider.usage(since: .distantPast).count
            print("Probe \(source.vendor): quotas=\(report.snapshots.count), sessions=\(report.sessions.count), buckets=\(buckets), completions=\(report.completions.count), indexing=\(report.indexing != nil), notice=\(report.sourceNotices[source.vendor] ?? "none")")
        }
    }
}
