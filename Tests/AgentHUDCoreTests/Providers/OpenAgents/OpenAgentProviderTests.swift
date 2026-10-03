import AgentHUDSupport
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
                                     .init(client: "OpenCode", provider: "GLM", product: .api, region: .international),
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

    /// Kimi's current report. The fixtures are derived from Kimi Code's own reader of `/coding/v1/usages`
    /// (`parseManagedUsagePayload`, MoonshotAI/kimi-code 21406fb4) and its membership docs, not captured from an account.
    func testKimiReadsTheCurrentReportUnderTheEarlierWindowIds() throws {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        let earlier = try OpenAgentQuotaClient.parse(json(#"{"usage":{"limit":"2000","used":"400"},"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"limit":"200","used":"50"}}]}"#), credential: credential(), now: now)
        // A current plan drops the week and adds the month's total, of which the code share is a part.
        let current = try OpenAgentQuotaClient.parse(json(#"{"usages":{"limit_5h":{"used_ratio":0.25,"reset_time":"2026-09-08T05:00:00Z"},"limit_month_total":{"used_ratio":"0.4","reset_time":"2026-10-01T00:00:00Z"},"limit_month_code":{"used_ratio":0.1}},"boosterWallet":null,"membership":{"level":"Plus"}}"#), credential: credential(), now: now)
        XCTAssertEqual(current.windows.map(\.id), [earlier.windows[1].id, credential().pool.windowID("monthly")], "the 5 hours keep their window")
        XCTAssertEqual(current.windows.map(\.remaining), [75, 60])
        XCTAssertEqual(current.windows.map(\.duration), [18000, nil])
        XCTAssertEqual(current.windows.first?.reset, DateParsing.internet("2026-09-08T05:00:00Z"))
        XCTAssertEqual(current.windows.map(\.label), ["5-hour quota · Plus", "Monthly total quota · Plus"])
        XCTAssertEqual(earlier.windows.map(\.label), ["Weekly quota", "5-hour quota"], "an account that names no plan")
        XCTAssertEqual(current.windows.map(\.shortLabel), ["5h", "Monthly"])
        // An older plan in the current report keeps its week, under the week's window.
        let older = try OpenAgentQuotaClient.parse(json(#"{"usages":{"limit_5h":{"used_ratio":0.25},"limit_7d":{"used_ratio":0.2}},"usage":{"limit":"2000","used":"1000"}}"#), credential: credential(), now: now)
        XCTAssertEqual(Set(older.windows.map(\.id)), Set(earlier.windows.map(\.id)))
        XCTAssertEqual(older.windows.map(\.remaining), [75, 80], "the current report is read alone where it gives a window")
        let fallback = try OpenAgentQuotaClient.parse(json(#"{"usages":{},"usage":{"limit":"2000","used":"400"}}"#), credential: credential(), now: now)
        XCTAssertEqual(fallback.windows.map(\.id), [earlier.windows[0].id])
    }

    func testGoOfficialUsageReadsResetTimestampsWithoutChangingPercentagesOrPeriods() throws {
        let root = try json(#"""
        {"usage":{
          "rolling":{"status":"ok","percent":0.5,"resetsAt":"2026-09-07T16:54:20.000Z"},
          "weekly":{"status":"ok","percent":1,"resetsAt":"2026-09-14T16:53:20.000Z"},
          "monthly":{"status":"ok","percent":20,"resetsAt":"2026-10-07T16:53:20.000Z"}
        }}
        """#)
        let result = try OpenAgentQuotaClient.parse(root, credential: credential(.go), now: now)
        XCTAssertEqual(result.windows.map(\.remaining), [99.5, 99, 80])
        XCTAssertEqual(result.windows.map(\.reset), [60, 604800, 2592000].map { now.addingTimeInterval($0) })
        XCTAssertEqual(result.windows.map(\.duration), [18000, 604800, nil])
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

    /// GLM credit plans. The fixture is derived from Zhipu's Coding Plan docs (credits over 5 hours and a week, MCP
    /// calls drawn from them) and the quota endpoint's fields, not captured from an account.
    func testGLMCreditPlansAreNamedInZhipusWordsAndTokenLimitsKeepTheirs() throws {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        let credits = try OpenAgentQuotaClient.parse(json(#"{"success":true,"code":200,"data":{"planName":"Pro","limits":[{"type":"CREDIT_LIMIT","unit":3,"number":5,"usage":12000,"currentValue":3000,"remaining":9000,"percentage":25,"nextResetTime":1788810000000},{"type":"CREDIT_LIMIT","unit":6,"number":1,"usage":60000,"currentValue":6000,"remaining":54000,"percentage":10}]}}"#), credential: credential(.glmGlobal), now: now)
        XCTAssertEqual(credits.windows.map(\.label), ["5-hour credits · Pro", "Weekly credits · Pro"])
        XCTAssertEqual(credits.windows.map(\.remaining), [75, 90])
        XCTAssertEqual(credits.windows.map(\.duration), [18000, 604800])
        XCTAssertEqual(credits.windows.map(\.id), ["CREDIT_LIMIT:3:5", "CREDIT_LIMIT:6:1"].map(credential(.glmGlobal).pool.windowID))
        XCTAssertEqual(credits.plan, "Pro")
        let tokens = try OpenAgentQuotaClient.parse(json(#"{"success":true,"code":200,"data":{"planName":"Pro","limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":25},{"type":"TIME_LIMIT","unit":5,"number":1,"percentage":5}]}}"#), credential: credential(.glmGlobal), now: now)
        XCTAssertEqual(tokens.windows.map(\.label), ["5-hour limit · Pro", "MCP usage (1 month) · Pro"])
    }

    func piLines(session: String = "original", entry: String = "message-a", provider: String = "openai-codex", model: String = "model-x") -> String {
        """
        {"type":"session","id":"\(session)","cwd":"/workspace","timestamp":"2026-09-07T00:00:00Z"}
        {"type":"message","id":"\(entry)","timestamp":"2026-09-07T00:00:01Z","message":{"role":"assistant","provider":"\(provider)","model":"\(model)","stopReason":"stop","usage":{"input":10,"output":20,"reasoning":5,"cacheRead":30,"cacheWrite":4,"cost":{"total":0.01}}}}
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
        XCTAssertNil(a.events[0].attribution?.pool)
        XCTAssertTrue(a.completions.isEmpty)
        XCTAssertTrue(a.turns.isEmpty)
    }

    func testCallsArePricedByTheModelTheyNamedThroughTheVendorsOwnService() async throws {
        let luna = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(model: "gpt-5.6-luna").utf8), path: "/a.jsonl").first)
        let route = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(provider: "openai", model: "gpt-5.6-luna").utf8), path: "/b.jsonl").first)
        let gateway = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(provider: "openrouter", model: "gpt-5.6-luna").utf8), path: "/c.jsonl").first)
        let call = luna.events[0]
        XCTAssertNotEqual(call.agentId, route.events[0].agentId, "each route stays its own consumer")
        let kinds = TokenKinds(tokensIn: call.tokensIn, tokensOut: call.tokensOut, cacheRead: call.cacheReadTokens, cacheWrite: call.cacheWriteTokens)
        let price = try XCTUnwrap(ModelCatalog.cost(agentId: "codex-model:gpt-5.6-luna", kinds: kinds))
        XCTAssertEqual(ModelCatalog.cost(agentId: call.agentId, kinds: kinds), price, "the same model costs the same whichever client called it")
        XCTAssertEqual(ModelCatalog.cost(agentId: route.events[0].agentId, kinds: kinds), price, "OpenAI's API and its ChatGPT plan are both OpenAI's")
        XCTAssertNil(ModelCatalog.cost(agentId: gateway.events[0].agentId, kinds: kinds), "a gateway sells the model at its own price")
        let ledger = UsageLedger.inMemory(), now = self.now
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in .init(sessions: [luna]) }, fetchQuota: { _, _ in ProviderQuota() },
                                              history: QuotaHistoryStore(), clock: { now }, ledger: ledger)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        let usage = try await ledger.sessionUsage(report.sessions.map(SessionUsageRequest.init))
        XCTAssertEqual(usage[luna.id]?.listCost, price.amount, "the session's row and page show it")
        let kimi = try XCTUnwrap(OpenAgentParser.kimi(Data(#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usageScope":"turn","time":1788800001000,"usage":{"inputOther":10,"output":5}}"#.utf8),
                                                      path: "/.kimi-code/sessions/work/session/agents/main/wire.jsonl").first)
        XCTAssertNil(ModelCatalog.model(for: kimi.events[0].agentId), "Kimi Code's plan id follows whichever model Moonshot ships")
    }

    /// Consumers are named by the catalog, from their ids: a route the model already shows is left out, and Kimi Code's
    /// plan model reads as Kimi's product.
    func testConsumersAreNamedByTheCatalog() async throws {
        let wire = #"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usageScope":"turn","time":1788800001000,"usage":{"inputOther":10,"output":5}}"#
        let kimi = try XCTUnwrap(OpenAgentParser.kimi(Data(wire.utf8), path: "/.kimi-code/sessions/work/session/agents/main/wire.jsonl").first)
        let plan = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(provider: "kimi-coding", model: "kimi-for-coding").utf8), path: "/a.jsonl").first)
        let other = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(session: "other", provider: "openai-codex", model: "gpt-5.6-luna").utf8), path: "/b.jsonl").first)
        let now = self.now
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in .init(sessions: [kimi, plan, other]) },
                                              fetchQuota: { _, _ in ProviderQuota() }, history: QuotaHistoryStore(), clock: { now })
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: report.consumers.map { ($0.id, $0.name) }), [
            "kimi-model:kimi-code/kimi-for-coding#kimi-code": "Kimi For Coding",
            "pi-model:kimi-for-coding#kimi-coding": "Kimi For Coding",
            "pi-model:gpt-5.6-luna#openai-codex": "gpt-5.6-luna · openai-codex",
        ])
    }

    func testASessionRecordedUnderTheHashedIdIsPricedWhole() async throws {
        let luna = try XCTUnwrap(OpenAgentParser.pi(Data(piLines(model: "gpt-5.6-luna").utf8), path: "/a.jsonl").first)
        let ledger = UsageLedger.inMemory(), now = self.now
        // Consumers were once a hash of the route's provider and model, then the model and a hash of the provider.
        let hashed = "pi-model:" + RecordCoding.hash(["openai-codex", "gpt-5.6-luna"])
        let routeHashed = "pi-model:gpt-5.6-luna#" + RecordCoding.hash(["openai-codex"])
        try await ledger.write { try $0.replace(source: OpenAgentUsageProvider.source, contribution: luna.id, events: [
            UsageLedger.Event(key: "earlier", timestamp: now.addingTimeInterval(-20 * 86400), agentId: hashed, tokensIn: 1_000, tokensOut: 10),
            UsageLedger.Event(key: "later", timestamp: now.addingTimeInterval(-10 * 86400), agentId: routeHashed, tokensIn: 500, tokensOut: 5),
        ]) }
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in .init(sessions: [luna]) }, fetchQuota: { _, _ in ProviderQuota() },
                                              history: QuotaHistoryStore(), clock: { now }, ledger: ledger)
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        let buckets = try await ledger.buckets(since: now.addingTimeInterval(-30 * 86400))
        XCTAssertEqual(Set(buckets.map(\.agentId)), [luna.events[0].agentId], "no unpriced twin of the same model")
        let usage = try await ledger.sessionUsage(report.sessions.map(SessionUsageRequest.init))
        XCTAssertEqual(usage[luna.id]?.calls, 3)
        XCTAssertNotNil(usage[luna.id]?.listCost, "the calls recorded before are priced too")
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
            fetchQuota: { _, _ in ProviderQuota() }, history: QuotaHistoryStore(),
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

    func testOpenCodeReadsMoreRepliesThanOneStatementMayReturn() throws {
        let url = try temp().appendingPathComponent("opencode.db")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        func sql(_ text: String) { XCTAssertEqual(sqlite3_exec(db, text, nil, nil, nil), SQLITE_OK) }
        sql("CREATE TABLE session(id TEXT PRIMARY KEY, title TEXT, directory TEXT)")
        sql("CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, data TEXT)")
        sql("INSERT INTO session VALUES ('s', 'Long run', '/workspace')")
        // A reply's record can carry the whole system prompt, which is never read.
        let prompt = String(repeating: "x", count: 200)
        sql("BEGIN")
        for index in 0..<10_500 {
            sql(#"INSERT INTO message VALUES ('\#(String(format: "msg_%06d", index))', 's', '{"role":"assistant","system":["\#(prompt)"],"modelID":"model","providerID":"p","time":{"created":\#(1_788_800_000_000 + index)},"tokens":{"input":1,"output":1}}')"#)
        }
        sql("COMMIT")
        let sessions = try OpenAgentParser.openCodeSQLite(url)
        XCTAssertEqual(Set(sessions.flatMap(\.events).compactMap(\.eventID)).count, 10_500, "every reply once, across pages")
        XCTAssertEqual(try OpenAgentParser.openCodeSQLite(url, since: Date(timeIntervalSince1970: 1_788_800_010_000.0 / 1000)).count, 500)
    }

    func testTitlesComeFromWhatEachClientKeeps() throws {
        // OpenCode keeps its placeholder name when the title call fails.
        let reply = #"{"role":"assistant","modelID":"m","providerID":"p","time":{"created":1788800000000},"tokens":{"input":1,"output":1},"path":{"root":"/work/app"}}"#
        let failed = try OpenAgentParser.openCodeMessage(json(reply), id: "m", sessionID: "s", path: "/m.json", title: "New session - 2026-09-28T07:46:18.123Z")
        XCTAssertEqual(failed?.title, "app")
        XCTAssertEqual(try OpenAgentParser.openCodeMessage(json(reply), id: "m", sessionID: "s", path: "/m.json", title: "News update query")?.title, "News update query")
        // Pi: the name last given, else the first message.
        let prompt = #"{"type":"message","id":"u","timestamp":"2026-09-07T00:00:00Z","message":{"role":"user","content":[{"type":"text","text":"Fix the parser\nplease"}]}}"#
        XCTAssertEqual(try OpenAgentParser.pi(Data((piLines() + "\n" + prompt).utf8), path: "/a").first?.title, "Fix the parser")
        let named = piLines() + "\n" + prompt + "\n" + #"{"type":"session_info","id":"i","name":"Parser work"}"#
        XCTAssertEqual(try OpenAgentParser.pi(Data(named.utf8), path: "/a").first?.title, "Parser work")
        XCTAssertEqual(try OpenAgentParser.pi(Data((named + "\n" + #"{"type":"session_info","id":"j","name":""}"#).utf8), path: "/a").first?.title,
                       "Fix the parser", "an empty name clears the one given before")
        XCTAssertEqual(try OpenAgentParser.pi(Data(piLines().utf8), path: "/a").first?.titleSource, .placeholder)
        // Kimi Code keeps the title in the session's state.json; sub-agents keep the client's name.
        let session = try temp().appendingPathComponent("sessions/wd/session")
        try FileManager.default.createDirectory(at: session.appendingPathComponent("agents/main"), withIntermediateDirectories: true)
        try #"{"title":"hello?","titleKind":"replaceable","isCustomTitle":false}"#.write(to: session.appendingPathComponent("state.json"), atomically: true, encoding: .utf8)
        let usage = #"{"type":"usage.record","model":"kimi-for-coding","usageScope":"turn","time":1788800001000,"usage":{"inputOther":10,"output":5}}"#
        XCTAssertEqual(try OpenAgentParser.kimi(Data(usage.utf8), path: session.appendingPathComponent("agents/main/wire.jsonl").path).first?.title, "hello?")
        XCTAssertEqual(try OpenAgentParser.kimi(Data(usage.utf8), path: session.appendingPathComponent("agents/child/wire.jsonl").path).first?.title, "Kimi")
    }

    func testSameRequestCannotMergeAcrossPools() {
        let a = credential().pool, b = credential(key: "another-account").pool
        func event(_ pool: BillingPool) -> UsageEvent {
            .init(timestamp: now, agentId: "model", tokensIn: 1, tokensOut: 2, eventID: "request",
                  attribution: .init(client: "Pi", providerID: "kimi-code", pool: pool))
        }
        XCTAssertEqual(UsageAggregation.usageUnion([[event(a)], [event(a)], [event(b)]]).count, 2)
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



    func testAQuotaReadingThatFailsKeepsTheAccountCurrentAtItsLastReading() async throws {
        let calls = Fetches(), key = credential()
        final class Clock: @unchecked Sendable { var now = Date(timeIntervalSince1970: 1_788_800_000) }
        let clock = Clock(), first = clock.now
        let provider = OpenAgentUsageProvider(credentials: { [key] }, sessions: { _ in .init() }, fetchQuota: { c, _ in
            await calls.record()
            guard await calls.count == 1 else { throw ProviderHTTPError(status: 503) }
            return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: 70)], plan: "Allegretto")
        }, history: QuotaHistoryStore(), clock: { clock.now })
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        clock.now = first.addingTimeInterval(UsageRefresh.accountRequestSpacing + 1)
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        let account = try XCTUnwrap(report.accounts?["Kimi"]?.first, "a failed reading is not a sign-out")
        XCTAssertTrue(account.isCurrent)
        XCTAssertEqual(account.observedAt, first, "it keeps the time of the reading that succeeded")
        XCTAssertEqual(account.plan, "Allegretto")
        XCTAssertNotNil(account.quotaNotice)
    }

    func testAPoolWhoseReadingFailsHoldsBackOnlyItsOwnRows() async throws {
        final class Clock: @unchecked Sendable { var now = Date(timeIntervalSince1970: 1_788_800_000) }
        let clock = Clock(), first = clock.now
        let failing = credential(key: "failing"), working = credential(key: "working", client: "Pi")
        let provider = RetainedUsageProvider(provider: OpenAgentUsageProvider(credentials: { [failing, working] }, sessions: { _ in .init() },
            fetchQuota: { c, now in
                if c.token == failing.token, now > first { throw ProviderHTTPError(status: 503) }
                return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: now > first ? 40 : 70)])
            }, history: QuotaHistoryStore(), clock: { clock.now }))
        _ = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        clock.now = first.addingTimeInterval(UsageRefresh.accountRequestSpacing + 1)
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        let failed = try XCTUnwrap(report.discoveredAgents.first { $0.billingPool == failing.pool })
        let read = try XCTUnwrap(report.discoveredAgents.first { $0.billingPool == working.pool })
        XCTAssertEqual(report.snapshot(for: failed.id)?.remainingPct, 70, "the failed pool keeps its last reading")
        XCTAssertNotNil(report.quotaNotice(for: failed))
        XCTAssertEqual(report.accounts?["Kimi"]?.first { $0.account.id == failed.account?.id }?.readingIssue?.kind, .readFailed)
        XCTAssertEqual(report.snapshot(for: read.id)?.remainingPct, 40)
        XCTAssertNil(report.quotaNotice(for: read), "the vendor's other pool is not held back")
        XCTAssertNil(report.quotaNotice(vendor: "Kimi"))
        XCTAssertNotNil(report.sourceNotices["Kimi"], "the failure is still shown under its client")
    }

    func testProviderResolvesDifferentCredentialsBeforeFetchingTheirSharedQuota() async throws {
        let calls = Fetches(), now = now
        let first = credential(key: "first"), second = credential(key: "second", client: "Pi")
        let provider = OpenAgentUsageProvider(credentials: { [first, second] }, sessions: { _ in .init() }, fetchQuota: { c, _ in
            await calls.record()
            return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: 70)], plan: "Allegretto")
        }, history: QuotaHistoryStore(), identify: { c in
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

    func testOpenCodeReadsEachSessionFromTheTableHoldingItsReplies() throws {
        let url = try temp().appendingPathComponent("opencode.db")
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        func reply(_ id: String, _ session: String) -> String {
            #"{"id":"\#(id)","sessionID":"\#(session)","role":"assistant","modelID":"glm-5.1","providerID":"zai-coding-plan","time":{"created":1788800000000},"tokens":{"input":10,"output":1}}"#
        }
        let statements = [
            "CREATE TABLE session(id TEXT, title TEXT, directory TEXT)",
            "CREATE TABLE message(id TEXT, session_id TEXT, data TEXT)",
            "CREATE TABLE session_message(id TEXT, session_id TEXT, type TEXT, data TEXT)",
            "INSERT INTO session VALUES ('old', 'Old kind', '/a'), ('new', 'New kind', '/b')",
            "INSERT INTO message VALUES ('m1', 'old', '\(reply("m1", "old"))'), ('m2', 'new', '\(reply("m2", "new"))')",
            // A switch comes before any reply of the newer kind; that reply names another id than the same one in `message`.
            #"INSERT INTO session_message VALUES ('e1','old','model-switched','{"time":{"created":1788800000000}}')"#,
            #"INSERT INTO session_message VALUES ('e2','new','assistant','{"model":{"id":"glm-5.1","providerID":"zai-coding-plan"},"time":{"created":1788800000000},"tokens":{"input":10,"output":1}}')"#,
        ]
        for statement in statements { XCTAssertEqual(sqlite3_exec(db, statement, nil, nil, nil), SQLITE_OK) }
        let sessions = try OpenAgentParser.openCodeSQLite(url)
        XCTAssertEqual(sessions.map(\.title).sorted(), ["New kind", "Old kind"])
        XCTAssertEqual(sessions.flatMap(\.events).compactMap(\.eventID).sorted(), ["opencode:e2", "opencode:m1"], "a reply counts once")
    }

    private struct FixedProvider: UsageProvider {
        let report: UsageReport
        func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport { report }
    }
    func testCombinedProviderUsesLatestSharedQuotaAndUnionsConsumersWithoutAddingCopies() async throws {
        let pool = credential().pool, id = pool.windowID("weekly")
        let agent = AgentDescriptor(id: id, vendor: "Kimi", model: "Weekly", source: "fixture", enabled: true, billingPool: pool)
        func report(remaining: Double, at: Date, client: String, eventID: String) -> UsageReport {
            .init(generatedAt: at, snapshots: [.init(agentId: id, remainingPct: remaining, updatedAt: at)], sessions: [],
                  discoveredAgents: [agent], consumerIdsByQuota: [id: [client]])
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
    }

    func testAPIBalancesKeepTheLatestObservationAndCurrency() {
        let api = BillingPool(provider: "GLM", realm: "CN", product: .api, scope: "a", evidence: .account, entitlement: "api")
        let period = Date(timeIntervalSince1970: (now.timeIntervalSince1970 / 900).rounded(.down) * 900)
        let old = APIBilling(vendor: "GLM", balances: [.init(currency: "CNY", total: 10, granted: 0, toppedUp: 10)], isAvailable: true,
            updatedAt: now.addingTimeInterval(-1), costs: [CostBucket(start: period, amounts: ["CNY": 1])], notice: nil, billingPool: api)
        let latest = APIBilling(vendor: "GLM", balances: [.init(currency: "CNY", total: 8, granted: 0, toppedUp: 8)], isAvailable: true,
            updatedAt: now, costs: [CostBucket(start: period, amounts: ["CNY": 2])], notice: nil, billingPool: api)
        let merged = CombinedUsageProvider.mergeBilling([old, latest])
        XCTAssertEqual(merged.count, 1)
        XCTAssertEqual(merged[0].balances.first?.total, 8) // Current balance is never added to an older observation.
        XCTAssertEqual(merged[0].costs.count, 1)
        XCTAssertEqual(merged[0].estimatedCost(currency: "CNY"), 2)
        XCTAssertNil(merged[0].estimatedCost(currency: "USD"))
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
        let ledger = UsageLedger.inMemory()
        let provider = OpenAgentUsageProvider(credentials: { [key, otherClient] }, sessions: { _ in .init(sessions: session) }, fetchQuota: { c, _ in
            await calls.record()
            return .init(windows: [.init(id: c.pool.windowID("weekly"), label: "Weekly", remaining: 70)])
        }, history: QuotaHistoryStore(), clock: { now }, ledger: ledger)
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 168)
        let count = await calls.count
        XCTAssertEqual(count, 1, "two credentials of one pool read it once")
        XCTAssertEqual(report.snapshots.count, 1)
        XCTAssertTrue(report.consumerIdsByQuota.values.allSatisfy(\.isEmpty))
        let recorded = try await ledger.buckets(since: .distantPast)
        XCTAssertEqual(recorded.count, 1)
    }
}
