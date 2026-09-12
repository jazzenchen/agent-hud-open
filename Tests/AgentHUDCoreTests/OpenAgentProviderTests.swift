import XCTest
import SQLite3
@testable import AgentHUDCore

final class OpenAgentProviderTests: XCTestCase {
    func testAPIServiceDiscoverySeparatesKeysOAuthPlansAndProxyOverrides() throws {
        let home = try temp()
        try write(#"{"anthropic":{"type":"api","key":"fixture"},"openai":{"type":"oauth","access":"fixture"},"kimi-for-coding":{"type":"api","key":"fixture"},"deepseek":{"type":"api","key":"fixture"},"zai-coding-plan":{"type":"api","key":"fixture"}}"#,
                  to: home.appendingPathComponent(".local/share/opencode/auth.json"))
        try write(#"{"provider":{"deepseek":{"options":{"baseURL":"https://proxy.example/v1"}},"zai-coding-plan":{"options":{"baseURL":"https://api.z.ai/api/paas/v4"}}}}"#,
                  to: home.appendingPathComponent(".config/opencode/opencode.json"))
        try write(#"{"openai":{"type":"api_key","key":"fixture"},"anthropic":{"type":"api_key","key":"!do-not-execute"},"kimi-coding":{"type":"oauth","access":"fixture"},"zai":{"type":"api_key","key":"fixture"}}"#,
                  to: home.appendingPathComponent(".pi/agent/auth.json"))
        let actual = AgentAPIServiceDiscovery.discover(home: home, environment: [:])
        XCTAssertEqual(Set(actual), [.init(client: "OpenCode", provider: "Anthropic", product: .api),
                                     .init(client: "OpenCode", provider: "GLM", product: .api),
                                     .init(client: "Pi", provider: "OpenAI", product: .api)])
        let encoded = String(decoding: try JSONEncoder().encode(actual), as: UTF8.self)
        XCTAssertFalse(encoded.contains("fixture"))
        XCTAssertFalse(encoded.contains("proxy.example"))
        XCTAssertTrue(AgentAPIServiceDiscovery.discover(home: try temp(), environment: [:]).isEmpty)
    }

    let now = Date(timeIntervalSince1970: 1_788_800_000)
    func json(_ text: String) throws -> ProviderJSON { try ProviderJSON.read(Data(text.utf8)) }
    func credential(_ service: OpenAgentCredential.Service = .kimi, key: String = "fixture-key", client: String = "Kimi") -> OpenAgentCredential {
        OpenAgentCredentials.credential(service, token: key, client: client)
    }
    func temp() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }
    func write(_ value: String, to file: URL) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(value.utf8).write(to: file)
    }

    func testSharedCredentialMergesClientsButKeepsRegionProductAndScopeSeparate() throws {
        let kimi = credential(), open = credential(client: "OpenCode"), pi = credential(client: "Pi")
        let merged = OpenAgentCredentials.merge([kimi, open, pi])
        XCTAssertEqual(merged.count, 1)
        XCTAssertEqual(merged.first?.clients, ["Kimi", "OpenCode", "Pi"])
        XCTAssertEqual(OpenAgentCredentials.merge([credential(.glmChina), credential(.glmGlobal)]).count, 2)
        XCTAssertEqual(OpenAgentCredentials.merge([kimi, credential(key: "another-key")]).count, 2)
        let pool = kimi.pool
        let api = BillingPool(provider: pool.provider, realm: pool.realm, product: .api, scope: pool.scope, evidence: pool.evidence, entitlement: pool.entitlement)
        XCTAssertNotEqual(api.id, pool.id)
        let accountA = OpenAgentCredentials.credential(.kimi, token: "a", client: "Kimi", accountID: "verified-account")
        let accountB = OpenAgentCredentials.credential(.kimi, token: "b", client: "Pi", accountID: "verified-account")
        XCTAssertEqual(OpenAgentCredentials.merge([accountA, accountB]).count, 1)
        let team = OpenAgentCredentials.credential(.kimi, token: "a", client: "Pi", accountID: "verified-account", organization: "org", project: "project")
        XCTAssertNotEqual(team.pool.id, accountA.pool.id)
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(pool), as: UTF8.self).contains("fixture-key"))
    }

    func testCredentialsReadNativeFilesAndHonorAPIOrProxyOverrides() throws {
        let home = try temp()
        try write(#"{"kimi-for-coding":{"type":"api","key":"shared"},"zai-coding-plan":{"type":"api","key":"zai-key"}}"#,
                  to: home.appendingPathComponent(".local/share/opencode/auth.json"))
        try write(#"{"kimi-for-coding":{"type":"api_key","key":"shared"},"zhipuai-coding-plan":{"type":"api_key","key":"!execute-me"}}"#,
                  to: home.appendingPathComponent(".pi/agent/auth.json"))
        try write(#"{"provider":{"zai-coding-plan":{"options":{"baseURL":"https://api.z.ai/api/paas/v4"}}}}"#,
                  to: home.appendingPathComponent(".config/opencode/opencode.json"))
        let actual = OpenAgentCredentials.discover(home: home, environment: ["KIMI_CODE_API_KEY": "shared"], now: now)
        XCTAssertEqual(actual.count, 1)
        XCTAssertEqual(actual.first?.clients, ["Kimi", "OpenCode", "Pi"])
        XCTAssertNil(OpenAgentCredentials.service(provider: "kimi-code", baseURL: "https://api.moonshot.cn/v1"))
        XCTAssertNil(OpenAgentCredentials.service(provider: "kimi-code", baseURL: "https://proxy.example/coding"))
        XCTAssertNil(OpenAgentCredentials.service(provider: "opencode")) // Zen is not Go.
        XCTAssertEqual(OpenAgentCredentials.service(provider: "", baseURL: "https://open.bigmodel.cn/api/coding/paas/v4"), .glmChina)
    }

    func testExpiredKimiCredentialsAreNotRefreshedOrRewritten() throws {
        let home = try temp(), file = home.appendingPathComponent(".kimi-code/credentials/kimi-code.json")
        let raw = #"{"access_token":"expired","expires_at":1,"refresh_token":"do-not-use"}"#
        try write(raw, to: file)
        XCTAssertTrue(OpenAgentCredentials.discover(home: home, environment: [:], now: now).isEmpty)
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), raw)
        XCTAssertFalse(FileManager.default.fileExists(atPath: home.appendingPathComponent(".kimi-code/device_id").path))
    }

    func testKimiWindowsKeepWeeklyAndFiveHourIndependentAndDoNotFabricateMissingLimits() throws {
        let root = try json(#"{"usage":{"limit":"2000","used":"400","resetTime":"2026-09-09T00:00:00Z"},"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"limit":"200","used":"50"}}]}"#)
        let result = try OpenAgentQuotaClient.parse(root, credential: credential(), now: now)
        XCTAssertEqual(result.windows.map(\.remaining), [80, 75])
        XCTAssertEqual(result.windows.map(\.duration), [604800, 18000])
        XCTAssertNotEqual(result.windows[0].id, result.windows[1].id)
        XCTAssertEqual(try OpenAgentQuotaClient.parse(json(#"{"usage":{"limit":"2000","remaining":"1600"}}"#), credential: credential(), now: now).windows.count, 1)
        XCTAssertThrowsError(try OpenAgentQuotaClient.parse(json(#"{"usage":{"limit":"0","used":"1"}}"#), credential: credential(), now: now))
    }

    func testGoFractionsArePercentAndMonthlyHasNoFabricatedPeriod() throws {
        let root = try json(#"{"usage":{"rolling":{"percent":0.5,"resetInSec":60},"weekly":{"percent":1},"monthly":{"percent":20}}}"#)
        let result = try OpenAgentQuotaClient.parse(root, credential: credential(.go), now: now)
        XCTAssertEqual(result.windows.map(\.remaining), [99.5, 99, 80])
        XCTAssertEqual(result.windows[0].reset, now.addingTimeInterval(60))
        XCTAssertNil(result.windows[2].duration)
    }

    func testGLMRegionsAndMCPStaySeparateAndInvalidResetIsOmitted() throws {
        let raw = #"{"success":true,"code":200,"data":{"limits":[{"type":"CREDIT_LIMIT","unit":3,"number":5,"percentage":40,"usage":100,"currentValue":20,"remaining":75,"nextResetTime":1999999999999},{"type":"TIME_LIMIT","unit":5,"number":1,"percentage":5}]}}"#
        let cn = try OpenAgentQuotaClient.parse(json(raw), credential: credential(.glmChina), now: now)
        let global = try OpenAgentQuotaClient.parse(json(raw), credential: credential(.glmGlobal), now: now)
        XCTAssertEqual(cn.windows.map(\.remaining), [75, 95])
        XCTAssertTrue(cn.windows[1].label.contains("MCP"))
        XCTAssertNil(cn.windows[0].reset)
        XCTAssertNil(cn.windows[1].duration)
        XCTAssertNotEqual(cn.windows[0].id, global.windows[0].id)
        XCTAssertThrowsError(try OpenAgentQuotaClient.parse(json(#"{"success":false,"code":200,"data":{"limits":[]}}"#), credential: credential(.glmChina), now: now))
    }

    func piLines(session: String = "original", entry: String = "message-a", provider: String = "openai-codex") -> String {
        """
        {"type":"session","id":"\(session)","cwd":"/workspace","timestamp":"2026-09-07T00:00:00Z"}
        {"type":"message","id":"\(entry)","timestamp":"2026-09-07T00:00:01Z","message":{"role":"assistant","provider":"\(provider)","model":"model-x","stopReason":"stop","usage":{"input":10,"output":20,"reasoning":5,"cacheRead":30,"cacheWrite":4,"cost":{"total":0.01}}}}
        """
    }
    func testPiForksDeduplicateButDifferentRequestsAndProviderRoutesRemainDistinct() throws {
        let a = try XCTUnwrap(OpenAgentParser.pi(Data(piLines().utf8), path: "/a.jsonl").first)
        let copy = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(session: "fork").utf8), path: "/b.jsonl").first)
        let other = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(entry: "message-b").utf8), path: "/c.jsonl").first)
        let route = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(provider: "openai").utf8), path: "/d.jsonl").first)
        XCTAssertEqual(UsageAggregation.usageUnion([a.events, copy.events]).count, 1)
        XCTAssertEqual(UsageAggregation.usageUnion([a.events, other.events, route.events]).count, 3)
        XCTAssertNotEqual(a.events[0].agentId, route.events[0].agentId)
        XCTAssertEqual(a.events[0].tokensIn, 14)
        XCTAssertEqual(a.events[0].tokensOut, 20) // reasoning is already within Pi output.
        XCTAssertEqual(a.events[0].cacheReadTokens, 30)
        XCTAssertEqual(a.events[0].attribution?.estimatedUSD, Decimal(string: "0.01"))
        XCTAssertNil(a.events[0].attribution?.pool)
        XCTAssertTrue(a.completions.isEmpty)
        XCTAssertTrue(a.turns.isEmpty)
    }

    func testKimiUsageRecordWinsOverStepSummaryAndLegacyStatusIsCumulative() throws {
        let modern = """
        {"type":"llm.request","model":"kimi-code/real-model","time":1788800000000}
        {"type":"usage.record","model":"__kimi_env_model__","usageScope":"turn","time":1788800001000,"usage":{"inputOther":10,"inputCacheRead":20,"inputCacheCreation":3,"output":5}}
        {"type":"step.end","time":1788800001000,"usage":{"inputOther":10,"output":5}}
        {"type":"usage.record","usageScope":"session","time":1788800002000,"usage":{"inputOther":999,"output":999}}
        {"type":"turn.ended","turnId":1,"reason":"completed","time":1788800003000}
        {"type":"turn.ended","turnId":2,"reason":"cancelled","time":1788800004000}
        """
        let parsed = try XCTUnwrap(OpenAgentParser.kimi(Data(modern.utf8), path: "/.kimi-code/sessions/work/session/agents/main/wire.jsonl").first)
        XCTAssertEqual(parsed.events.count, 1)
        XCTAssertEqual(parsed.events.first?.tokensIn, 13)
        XCTAssertEqual(parsed.models.values.first, "kimi-code/real-model")
        XCTAssertEqual(parsed.completions.count, 1)
        XCTAssertEqual(parsed.turns.map(\.state), [.completed, .ended])
        let sub = try XCTUnwrap(OpenAgentParser.kimi(Data(modern.utf8), path: "/.kimi-code/sessions/work/session/agents/child/wire.jsonl").first)
        XCTAssertTrue(sub.completions.isEmpty)
        let legacy = """
        {"timestamp":1788800000,"message":{"type":"StatusUpdate","payload":{"message_id":"same","token_usage":{"input_other":10,"output":5}}}}
        {"timestamp":1788800001,"message":{"type":"StatusUpdate","payload":{"message_id":"same","token_usage":{"input_other":10,"output":15}}}}
        """
        let old = try XCTUnwrap(OpenAgentParser.kimi(Data(legacy.utf8), path: "/.kimi/sessions/work/session/wire.jsonl").first)
        XCTAssertEqual(old.events.count, 1)
        XCTAssertEqual(old.events[0].tokensOut, 15)
        XCTAssertEqual(old.models.values.first, "Unknown")
    }

    func testKimiLoopEventsReachRunningReportBeforeUsageAndRetainTurnIdentityAtEnd() async throws {
        let start: Int64 = 1_788_800_000_000
        let path = "/.kimi-code/sessions/work/session/agents/main/wire.jsonl"
        let running = """
        {"type":"context.append_loop_event","time":\(start),"event":{"type":"step.begin","turnId":"0","step":0}}
        {"type":"context.append_loop_event","time":\(start + 1000),"event":{"type":"content.part","turnId":"0","step":0,"part":{"text":"private text"}}}
        """
        let item = try XCTUnwrap(OpenAgentParser.kimi(Data(running.utf8), path: path).first)
        XCTAssertTrue(item.events.isEmpty)
        let turn = try XCTUnwrap(item.turns.first)
        XCTAssertEqual(turn.state, .running)
        XCTAssertEqual(turn.startedAtMs, start)
        XCTAssertEqual(turn.observedAtMs, start + 1000)
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in .init(sessions: [item]) },
            fetchQuota: { _, _ in ProviderQuota() }, history: QuotaHistoryStore(fileURL: nil),
            clock: { Date(timeIntervalSince1970: Double(start + 2000) / 1000) })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.turns, [turn])
        XCTAssertEqual(report.sessions.first?.id, turn.sessionID)
        XCTAssertEqual(report.sessions.first?.isLive, true)
        XCTAssertEqual(report.consumers.first?.vendor, "Kimi")
        for reason in ["completed", "cancelled", "error"] {
            let ended = running + "\n" + #"{"type":"turn.ended","turnId":0,"reason":"REASON","time":1788800003000}"#.replacingOccurrences(of: "REASON", with: reason)
            let final = try XCTUnwrap(OpenAgentParser.kimi(Data(ended.utf8), path: path).first)
            XCTAssertEqual(final.turns.count, 1)
            XCTAssertEqual(final.turns.first?.id, turn.id)
            XCTAssertEqual(final.turns.first?.startedAtMs, start)
            XCTAssertEqual(final.turns.first?.state, reason == "completed" ? .completed : .ended)
            XCTAssertEqual(final.turns.first?.observedAtMs, start + 3000)
        }
        let child = try XCTUnwrap(OpenAgentParser.kimi(Data(running.utf8), path: path.replacingOccurrences(of: "/main/", with: "/child/")).first)
        XCTAssertTrue(child.turns.isEmpty)
    }

    func testPartialLastLineIsRetryableButInteriorCorruptionIsReported() throws {
        XCTAssertEqual(try OpenAgentParser.pi(Data((piLines() + "\n{unfinished").utf8), path: "/a").first?.events.count, 1)
        XCTAssertThrowsError(try OpenAgentParser.pi(Data((piLines() + "\n{broken}\n").utf8), path: "/a"))
        XCTAssertThrowsError(try OpenAgentParser.pi(Data(piLines().replacingOccurrences(of: "\"input\":10", with: "\"input\":-10").utf8), path: "/a"))
    }

    func testMalformedHugeCountersAndDurationsCannotOverflow() throws {
        XCTAssertThrowsError(try OpenAgentParser.pi(Data(piLines().replacingOccurrences(of: "\"input\":10", with: "\"input\":9223372036854775807").utf8), path: "/a"))
        XCTAssertThrowsError(try OpenAgentQuotaClient.parse(json(#"{"limits":[{"window":{"duration":1e100,"timeUnit":"TIME_UNIT_HOUR"},"detail":{"limit":10,"used":1}}]}"#), credential: credential(), now: now))
    }

    func testOpenCodeSQLiteWALAndLegacyJSONHaveSameEventIdentity() throws {
        let home = try temp(), dbURL = home.appendingPathComponent("opencode.db")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(dbURL.path, &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        func sql(_ text: String) { XCTAssertEqual(sqlite3_exec(db, text, nil, nil, nil), SQLITE_OK) }
        sql("PRAGMA journal_mode=WAL")
        sql("CREATE TABLE session(id TEXT, title TEXT, directory TEXT)")
        sql("CREATE TABLE message(id TEXT, session_id TEXT, data TEXT)")
        sql("INSERT INTO session VALUES ('s', 'Task', '/workspace')")
        let raw = #"{"id":"m","sessionID":"s","role":"assistant","modelID":"model","providerID":"zhipuai-coding-plan","time":{"created":1788800000000},"tokens":{"input":10,"output":20,"reasoning":2,"cache":{"read":30,"write":4}},"cost":0.2}"#
        sql("INSERT INTO message VALUES ('m', 's', '\(raw)')")
        let sqlite = try XCTUnwrap(OpenAgentParser.openCodeSQLite(dbURL).first)
        let legacy = try XCTUnwrap(OpenAgentParser.openCodeMessage(json(raw), id: "m", sessionID: "s", path: "/m.json"))
        XCTAssertEqual(UsageAggregation.usageUnion([sqlite.events, legacy.events]).count, 1)
        XCTAssertEqual(sqlite.events[0].tokensIn, 14)
        XCTAssertEqual(sqlite.events[0].tokensOut, 22)
        XCTAssertEqual(sqlite.title, "Task")
        XCTAssertTrue(try OpenAgentParser.openCodeSQLite(dbURL, since: now.addingTimeInterval(1)).isEmpty)
        XCTAssertNil(sqlite.events[0].attribution?.pool)
    }




    func testKimiNativeRegionalSlotsAndExplicitAPIEndpointsStaySeparate() throws {
        let home = try temp()
        XCTAssertEqual(OpenAgentCredentials.kimiStorageName(.kimiGlobal), "kimi-code-env-0e4f99c69cc27850")
        for name in ["kimi-code", "kimi-code-env-0e4f99c69cc27850"] {
            try write(#"{"access_token":"same-fixture-token","expires_at":1999999999}"#,
                      to: home.appendingPathComponent(".kimi-code/credentials/\(name).json"))
        }
        let both = OpenAgentCredentials.discover(home: home, environment: [:], now: now)
        XCTAssertEqual(both.count, 2)
        XCTAssertEqual(Set(both.map { $0.pool.realm }), ["CN", "International"])
        XCTAssertEqual(Set(both.compactMap { $0.endpoint.host }), ["api.kimi.com", "api.kimi.ai"])
        XCTAssertFalse(FileManager.default.fileExists(atPath: home.appendingPathComponent(".kimi-code/device_id").path))
        let global = OpenAgentCredentials.discover(home: home, environment: ["KIMI_CODE_BASE_URL": "https://api.kimi.ai/coding/v1"], now: now)
        XCTAssertEqual(global.count, 1)
        XCTAssertEqual(global.first?.service, .kimiGlobal)
        XCTAssertTrue(OpenAgentCredentials.discover(home: home, environment: ["KIMI_CODE_BASE_URL": "https://custom.example/coding/v1"], now: now).isEmpty)
    }

    func testJSONCOverrideCannotBeMistakenForNativeProvider() throws {
        let home = try temp()
        try write(#"{"kimi-for-coding":{"type":"api","key":"fixture-key"}}"#,
                  to: home.appendingPathComponent(".local/share/opencode/auth.json"))
        try write("""
        { // official JSONC configuration with an explicit proxy
          "provider": {"kimi-for-coding": {"options": {"baseURL": "https://proxy.example/coding",},},},
        }
        """, to: home.appendingPathComponent(".config/opencode/opencode.jsonc"))
        XCTAssertTrue(OpenAgentCredentials.discover(home: home, environment: [:], now: now).isEmpty)
    }

    func testKimiOfficialProfileProvesIdentityAcrossKeysWithinOneDeployment() async throws {
        let client = OpenAgentQuotaClient(http: ProviderHTTP(send: { request in
            XCTAssertEqual(request.url?.path, "/coding/v1/me")
            XCTAssertEqual(request.httpMethod, "GET")
            XCTAssertNil(request.httpBody)
            XCTAssertEqual(request.timeoutInterval, 8)
            return Data(#"{"user_id":"account-one","domain":1,"region":"REGION_CN","email":"never-persist@example.com"}"#.utf8)
        }))
        let a = try await client.identify(credential(key: "first"))
        let b = try await client.identify(credential(key: "second", client: "Pi"))
        let global = try await client.identify(credential(.kimiGlobal, key: "second"))
        XCTAssertEqual(a.pool, b.pool)
        XCTAssertEqual(a.pool.evidence, .account)
        XCTAssertNotEqual(a.pool, global.pool)
        let encoded = String(decoding: try JSONEncoder().encode(a.pool), as: UTF8.self)
        XCTAssertFalse(encoded.contains("account-one"))
        XCTAssertFalse(encoded.contains("never-persist"))
        let failed = OpenAgentQuotaClient(http: ProviderHTTP(send: { _ in throw ProviderHTTPError(status: 404) }))
        do {
            _ = try await failed.identify(credential())
            XCTFail("Identity failures must reach the provider rather than silently passing as identified")
        } catch { XCTAssertEqual((error as? ProviderHTTPError)?.status, 404) }
    }



    func testProviderResolvesDifferentCredentialsBeforeFetchingTheirSharedQuota() async throws {
        let calls = Fetches(), now = now
        let first = credential(key: "first"), second = credential(key: "second", client: "Pi")
        let provider = OpenAgentUsageProvider(credentials: { [first, second] }, sessions: { _ in .init() }, fetchQuota: { c, _ in
            await calls.record()
            return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: 70)], plan: "Allegretto")
        }, history: QuotaHistoryStore(fileURL: nil), identify: { c in
            OpenAgentCredentials.credential(c.service, token: c.token, client: c.clients.sorted()[0], accountID: "proven-account")
        }, clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        let count = await calls.count
        XCTAssertEqual(count, 1)
        XCTAssertEqual(report.snapshots.count, 1)
        XCTAssertEqual(report.discoveredAgents.first?.billingPool?.evidence, .account)
        XCTAssertEqual(report.discoveredAgents.first?.source, "Kimi, Pi")
        XCTAssertEqual(report.services?.map(\.client).sorted(), ["Kimi", "Pi"])
        XCTAssertEqual(Set(report.services?.compactMap { report.subscriptions[$0.accountID ?? ""] } ?? []), ["Allegretto"])
        XCTAssertEqual(Set(report.services?.compactMap(\.accountID) ?? []).count, 1)
    }


    func testOpenCodeV2UsesRowRoleAndNestedModelWithoutLegacyJSONRole() throws {
        let url = try temp().appendingPathComponent("opencode.db")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        let statements = [
            "CREATE TABLE session_v2(id TEXT, title TEXT, directory TEXT)",
            "CREATE TABLE session_message(id TEXT, session_id TEXT, type TEXT, data TEXT)",
            "INSERT INTO session_v2 VALUES ('s', 'V2 task', '/workspace')",
            #"INSERT INTO session_message VALUES ('m','s','assistant','{"model":{"id":"model-v2","providerID":"opencode-go"},"time":{"created":1788800000000},"tokens":{"input":10,"output":20}}')"#,
            #"INSERT INTO session_message VALUES ('u','s','user','{"time":{"created":1788800000000},"tokens":{"input":900,"output":900}}')"#,
        ]
        for statement in statements { XCTAssertEqual(sqlite3_exec(db, statement, nil, nil, nil), SQLITE_OK) }
        let sessions = try OpenAgentParser.openCodeSQLite(url)
        XCTAssertEqual(sessions.count, 1)
        XCTAssertEqual(sessions[0].models.values.first, "model-v2")
        XCTAssertEqual(sessions[0].events.first?.attribution?.providerID, "opencode-go")
    }

    private struct FixedProvider: UsageProvider {
        let report: UsageReport
        func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport { report }
    }
    func testCombinedProviderUsesLatestSharedQuotaAndUnionsConsumersWithoutAddingCopies() async throws {
        let pool = credential().pool, id = pool.windowID("weekly")
        let agent = AgentDescriptor(id: id, vendor: "Kimi", model: "Weekly", source: "fixture", enabled: true, billingPool: pool)
        func report(remaining: Double, at: Date, client: String, eventID: String) -> UsageReport {
            .init(generatedAt: at, snapshots: [.init(agentId: id, remainingPct: remaining, updatedAt: at)], sessions: [], history: [], activity: .empty, insights: .empty,
                  discoveredAgents: [agent], consumption: [.init(timestamp: now, agentId: "model", tokensIn: 10, tokensOut: 20, eventID: eventID)], consumerIdsByQuota: [id: [client]])
        }
        let a = report(remaining: 80, at: now.addingTimeInterval(-1), client: "Pi", eventID: "same")
        let b = report(remaining: 70, at: now, client: "Kimi", eventID: "same")
        let c = report(remaining: 70, at: now, client: "OpenCode", eventID: "different")
        let provider = CombinedUsageProvider([.init("Pi", FixedProvider(report: a)), .init("Kimi", FixedProvider(report: b)), .init("OpenCode", FixedProvider(report: c))])
        let result = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        XCTAssertEqual(result.snapshots.count, 1)
        XCTAssertEqual(result.snapshots[0].remainingPct, 70)
        XCTAssertEqual(result.discoveredAgents.count, 1)
        XCTAssertEqual(result.consumerIdsByQuota[id], ["Pi", "Kimi", "OpenCode"])
        XCTAssertEqual(result.consumption.count, 2)
    }


    func testPiProviderIDsHaveClientSpecificBillingMeaningAndOAuthStaysReadOnly() throws {
        XCTAssertNil(OpenAgentCredentials.service(provider: "zai", client: .opencode))
        XCTAssertEqual(OpenAgentCredentials.service(provider: "zai", client: .pi), .glmGlobal)
        XCTAssertEqual(OpenAgentCredentials.service(provider: "zai-coding-cn", client: .pi), .glmChina)
        let home = try temp(), file = home.appendingPathComponent(".pi/agent/auth.json")
        let auth = #"{"zai":{"type":"api_key","key":"same"},"zai-coding-cn":{"type":"api_key","key":"same"},"kimi-coding":{"type":"oauth","access":"access-fixture","refresh":"never-use","expires":1999999999000}}"#
        try write(auth, to: file)
        let found = OpenAgentCredentials.discover(home: home, environment: [:], now: now)
        XCTAssertEqual(found.count, 3)
        XCTAssertEqual(Set(found.map(\.service)), [.kimi, .glmChina, .glmGlobal])
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), auth)
        let custom = OpenAgentCredentials.discover(home: home, environment: ["KIMI_CODE_OAUTH_HOST": "https://custom.example"], now: now)
        XCTAssertFalse(custom.contains { $0.service == .kimi })
    }

    private actor Fetches {
        var count = 0
        func record() { count += 1 }
    }
    func testProviderFetchesSharedPoolOnceAndDoesNotAssignUnknownHistoricalUsage() async throws {
        let calls = Fetches(), key = credential(), otherClient = credential(client: "Pi"), now = now
        let session = try OpenAgentParser.pi(Data(piLines().utf8), path: "/a")
        let provider = OpenAgentUsageProvider(credentials: { [key, otherClient] }, sessions: { _ in .init(sessions: session) }, fetchQuota: { c, _ in
            await calls.record()
            return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: 70)])
        }, history: QuotaHistoryStore(fileURL: nil), clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        let count = await calls.count
        XCTAssertEqual(count, 1)
        XCTAssertEqual(report.snapshots.count, 1)
        XCTAssertTrue(report.consumerIdsByQuota.values.allSatisfy(\.isEmpty))
        XCTAssertEqual(report.consumption.count, 1)
    }
}
