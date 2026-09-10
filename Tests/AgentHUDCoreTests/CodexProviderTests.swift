import XCTest
@testable import AgentHUDCore

final class CodexProviderTests: XCTestCase {
    static let limits = #"{"rateLimits":{"limitId":"codex","primary":{"usedPercent":99}},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":3,"windowDurationMins":10080,"resetsAt":1789377360},"secondary":null,"planType":"prolite"},"codex_bengalfox":{"limitName":"GPT-5.3-Codex-Spark","primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":1788792308},"secondary":{"usedPercent":24,"windowDurationMins":10080,"resetsAt":1789379108}}}}"#

    private func decode(_ text: String = CodexProviderTests.limits) throws -> CodexRateLimits {
        try JSONDecoder().decode(CodexRateLimits.self, from: Data(text.utf8))
    }

    static let resets = #"{"availableCount":3,"credits":[{"id":"later","status":"available","expiresAt":1791092020},{"id":"unknown","status":"available","expiresAt":null},{"id":"first","status":"available","expiresAt":1789949974}]}"#
    static let limitsWithResets = String(limits.dropLast()) + ",\"rateLimitResetCredits\":" + resets + "}"

    func testResetCreditsDecodeUnixExpiryAndSortUnknownLast() throws {
        let limits = try decode(Self.limitsWithResets)
        let resets = try XCTUnwrap(limits.rateLimitResetCredits)
        XCTAssertEqual(resets.availableCount, 3)
        XCTAssertEqual(resets.creditsByExpiry.map(\.id), ["first", "later", "unknown"])
        XCTAssertEqual(resets.creditsByExpiry.first?.expirationDate, Date(timeIntervalSince1970: 1789949974))
        XCTAssertNil(resets.creditsByExpiry.last?.expirationDate)
        XCTAssertEqual(limits.rows.count, 3, "earned resets are not another quota window")
    }

    func testResetCreditsDistinguishUnavailableZeroAndPartialDetails() throws {
        XCTAssertNil(try decode().rateLimitResetCredits)
        XCTAssertNil(try decode(#"{"rateLimitResetCredits":null}"#).rateLimitResetCredits)
        let zero = try XCTUnwrap(decode(#"{"rateLimitResetCredits":{"availableCount":0,"credits":[]}}"#).rateLimitResetCredits)
        XCTAssertEqual(zero.availableCount, 0)
        XCTAssertEqual(zero.credits, [])
        for details in ["null", "[]", #"[{"id":"only-detail","expiresAt":null}]"#] {
            let resets = try XCTUnwrap(decode("{\"rateLimitResetCredits\":{\"availableCount\":5,\"credits\":\(details)}}").rateLimitResetCredits)
            XCTAssertEqual(resets.availableCount, 5, "the service can omit or cap details without reducing the balance")
            if details == "null" { XCTAssertNil(resets.credits) }
        }
    }

    func testActualMultiBucketResponseAndWeeklyPrimary() throws {
        let limits = try decode()
        XCTAssertEqual(limits.rows.count, 3, "missing secondary does not invent a 5-hour window")
        XCTAssertEqual(limits.rows.first?.id, "codex", "existing preferences survive discovery")
        XCTAssertEqual(limits.rows.first?.window.remainingPct, 97, "map takes precedence over legacy 99%")
        XCTAssertEqual(limits.rows.first?.weekly?.remainingPct, 97)
        XCTAssertEqual(limits.rows.last?.window.remainingPct, 76)
        XCTAssertEqual(limits.plan, "prolite")
        XCTAssertEqual(limits.rows.first?.window.resetAt?.timeIntervalSince1970, 1789377360)
    }

    func testLegacyEmptyAndMissingAreDistinct() throws {
        XCTAssertEqual(try decode(#"{"rateLimits":{"primary":{"usedPercent":25,"windowDurationMins":300}}}"#).rows.first?.window.remainingPct, 75)
        XCTAssertTrue(try decode(#"{"rateLimits":{"primary":{"usedPercent":25}},"rateLimitsByLimitId":{}}"#).rows.isEmpty)
        XCTAssertTrue(try decode(#"{"rateLimits":{"primary":null,"secondary":null}}"#).rows.isEmpty)
        XCTAssertThrowsError(try decode(#"{"rateLimits":{"primary":{"windowDurationMins":300}}}"#), "missing usage is not zero")
    }

    func testDesktopAndCLIHaveSameTokensAndImmediateEndState() throws {
        for (origin, source, expected) in [("Codex Desktop", "vscode", "Desktop"), ("codex_cli_rs", "cli", "CLI"), ("Codex Desktop", "exec", "CLI · exec")] {
            var t = CodexTranscript()
            ingest(&t, type: "session_meta", payload: ["id":"session", "cwd":"/project", "source":source, "originator":origin])
            ingest(&t, type: "turn_context", payload: ["model":"gpt-6-astra"])
            ingest(&t, payload: ["type":"task_started"])
            token(&t, input: 1000, cached: 800, output: 100)
            token(&t, input: 1000, cached: 800, output: 100)
            token(&t, input: 1500, cached: 1200, output: 160)
            XCTAssertEqual(t.client, expected)
            XCTAssertEqual(t.usage.reduce(0) { $0 + $1.input }, 300)
            XCTAssertEqual(t.usage.reduce(0) { $0 + $1.output }, 160)
            XCTAssertEqual(t.usage.count, 2, "repeated cumulative snapshots do not duplicate tokens")
            XCTAssertTrue(t.isLive(now: now, modifiedAt: now))
            ingest(&t, payload: ["type":"task_complete"])
            XCTAssertFalse(t.isLive(now: now, modifiedAt: now), "a recent write is not enough to resurrect an ended turn")
            ingest(&t, payload: ["type":"task_started"])
            XCTAssertTrue(t.isLive(now: now, modifiedAt: now))
            ingest(&t, payload: ["type":"turn_aborted"])
            XCTAssertFalse(t.isLive(now: now, modifiedAt: now))
        }
    }

    func testForkHistoryIsDeduplicatedButInternalModelUsageIsIncluded() {
        var t = CodexTranscript()
        ingest(&t, type: "session_meta", payload: ["id":"child", "timestamp":"2026-09-07T09:00:00Z", "source":["subagent":["thread_spawn":["parent_thread_id":"parent"]]]])
        token(&t, input: 1000, cached: 800, output: 100, at: "2026-09-07T08:00:00Z")
        token(&t, input: 1400, cached: 1000, output: 150)
        XCTAssertEqual(t.usage.count, 1)
        XCTAssertEqual(t.usage.first?.input, 200)
        XCTAssertEqual(t.usage.first?.output, 50)
        var guardian = CodexTranscript()
        ingest(&guardian, type: "session_meta", payload: ["id":"review", "source":["subagent":["other":"guardian"]]])
        token(&guardian, input: 10000, cached: 0, output: 1000)
        XCTAssertEqual(guardian.usage.first?.input, 10000)
        XCTAssertEqual(guardian.usage.first?.output, 1000)
    }

    func testIncrementalStoreHandlesPartialLineAndArchiveDuplicate() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("rollout-test.jsonl")
        let meta = line(type: "session_meta", payload: ["id":"same", "source":"cli"])
        let usage = line(payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":500, "cached_input_tokens":200, "output_tokens":50]]])
        try (meta + "\n" + String(usage.prefix(70))).write(to: file, atomically: true, encoding: .utf8)
        let cache = dir.appendingPathComponent("cache.json")
        let store = CodexTranscriptStore(roots: [dir], cacheURL: cache)
        let first = await store.index(since: .distantPast)
        XCTAssertEqual(first.sessions.count, 1)
        XCTAssertTrue(first.sessions[0].transcript.usage.isEmpty)
        let checkpoint = try Data(contentsOf: cache)
        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd(); try handle.write(contentsOf: Data((String(usage.dropFirst(70)) + "\n").utf8)); try handle.close()
        let second = await store.index(since: .distantPast)
        XCTAssertEqual(second.sessions[0].transcript.usage.first?.input, 300)
        XCTAssertEqual(second.sessions[0].transcript.usage.first?.event.cacheReadTokens, 200)
        XCTAssertEqual(try Data(contentsOf: cache), checkpoint, "frequent polls coalesce cache writes")
        try FileManager.default.copyItem(at: file, to: dir.appendingPathComponent("rollout-archived.jsonl"))
        let restored = CodexTranscriptStore(roots: [dir], cacheURL: cache)
        let third = await restored.index(since: .distantPast)
        XCTAssertEqual(third.sessions.count, 1)
        XCTAssertEqual(third.sessions[0].transcript.usage.count, 1)
    }

    func testClientPerformsHandshakeWithoutStartingSession() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let script = """
        #!/usr/bin/python3
        import sys,json,os
        for line in sys.stdin:
            request=json.loads(line)
            with open(os.path.join(os.path.dirname(__file__),'methods'),'a') as log: log.write(request['method']+'\\n')
            if request['method']=='initialize':
                print(json.dumps({'id':1,'result':{'userAgent':'test'}}),flush=True)
            elif request['method']=='account/rateLimits/read':
                print(json.dumps({'method':'account/rateLimits/updated','params':{}}),flush=True)
                print(json.dumps({'id':999,'result':{}}),flush=True)
                print('{"id":2,"result":\(Self.limitsWithResets)}',flush=True)
        """
        let exe = dir.appendingPathComponent("codex")
        try script.write(to: exe, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: exe.path)
        let result = try await CodexAppServerClient(executable: exe, dataDirectory: dir, timeout: 5).fetch()
        XCTAssertEqual(result.rows.count, 3)
        XCTAssertEqual(result.rateLimitResetCredits?.availableCount, 3)
        XCTAssertEqual(try String(contentsOf: dir.appendingPathComponent("methods"), encoding: .utf8), "initialize\ninitialized\naccount/rateLimits/read\n")
    }

    func testLocatorWorksWithoutDesktopOrWithoutCLI() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let apps = dir.appendingPathComponent("Applications")
        let cli = dir.appendingPathComponent(".bun/bin/codex")
        try FileManager.default.createDirectory(at: cli.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: cli, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: cli.path)
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: ""), cli)
        try FileManager.default.removeItem(at: cli)
        let desktop = apps.appendingPathComponent("Codex.app/Contents/Resources/codex")
        try FileManager.default.createDirectory(at: desktop.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: desktop, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: desktop.path)
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: ""), desktop)
    }

    func testQuotaFailureKeepsLocalSessions() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let contents = line(type: "session_meta", payload: ["id":"cli", "source":"cli", "cwd":"/project"]) + "\n" + line(payload: ["type":"task_started"]) + "\n"
        try contents.write(to: dir.appendingPathComponent("rollout-cli.jsonl"), atomically: true, encoding: .utf8)
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: CodexTranscriptStore(roots: [dir]), history: QuotaHistoryStore(fileURL: nil))
        let report = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(report.sessions.first?.client, "CLI")
        let session = try XCTUnwrap(report.sessions.first)
        XCTAssertTrue(report.consumerIdsByQuota["codex"]?.contains(session.agentId) == true)
        XCTAssertNil(report.sessions.first?.pctOfWindow)
        XCTAssertTrue(report.snapshots.isEmpty)
        XCTAssertEqual(report.sourceNotices["Codex"], "signed out")
        XCTAssertNil(report.codexResetCredits)
    }

    func testForecastUsesEachServicesWindowPeriodAndFullCurrentCycle() async throws {
        let now = self.now
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let weeklyReset = now.addingTimeInterval(2 * 86400).timeIntervalSince1970
        let sessionReset = now.addingTimeInterval(2 * 3600).timeIntervalSince1970
        let limits = try decode("""
        {"rateLimitsByLimitId": {
          "codex": {"primary": {"usedPercent": 40, "windowDurationMins": 10080, "resetsAt": \(weeklyReset)}},
          "spark": {"primary": {"usedPercent": 60, "windowDurationMins": 300, "resetsAt": \(sessionReset)}},
          "unknown": {"primary": {"usedPercent": 50, "resetsAt": \(sessionReset)}}
        }}
        """)
        let history = QuotaHistoryStore(fileURL: nil)
        await history.append([
            QuotaSample(agentId: "codex", timestamp: now.addingTimeInterval(-5 * 86400), remainingPct: 100),
            QuotaSample(agentId: "codex", timestamp: now.addingTimeInterval(-86400), remainingPct: 70),
            QuotaSample(agentId: "codex:spark:primary", timestamp: now.addingTimeInterval(-3 * 3600), remainingPct: 100),
            QuotaSample(agentId: "codex:spark:primary", timestamp: now.addingTimeInterval(-3600), remainingPct: 40),
            QuotaSample(agentId: "codex:unknown:primary", timestamp: now.addingTimeInterval(-3600), remainingPct: 80)
        ], now: now)
        let provider = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: [dir]),
                                          history: history, clock: { now })
        let report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshot(for: "codex")?.windowDuration, 7 * 86400)
        XCTAssertEqual(report.snapshot(for: "codex:spark:primary")?.windowDuration, 5 * 3600)
        let weekly = try XCTUnwrap(report.insightsByAgent["codex"])
        XCTAssertEqual(try XCTUnwrap(weekly.burnRatePctPerHour), 40.0 / 120, accuracy: 1e-9,
                       "the statistics range must not truncate the quota cycle")
        XCTAssertEqual(try XCTUnwrap(weekly.timeToExhaust), 180 * 3600, accuracy: 1e-6)
        let session = try XCTUnwrap(report.insightsByAgent["codex:spark:primary"])
        XCTAssertEqual(try XCTUnwrap(session.burnRatePctPerHour), 20, accuracy: 1e-9,
                       "an idle last hour still includes earlier consumption in this cycle")
        XCTAssertEqual(try XCTUnwrap(session.timeToExhaust), 2 * 3600, accuracy: 1e-6)
        XCTAssertNil(report.insightsByAgent["codex:unknown:primary"]?.burnRatePctPerHour)
    }

    private var now: Date { Date(timeIntervalSince1970: 1788771600) }
    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-codex-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
    private func line(type: String = "event_msg", payload: [String: Any], at: String = "2026-09-07T09:00:00Z") -> String {
        String(decoding: try! JSONSerialization.data(withJSONObject: ["type":type,"timestamp":at,"payload":payload], options: .sortedKeys), as: UTF8.self)
    }
    private func ingest(_ t: inout CodexTranscript, type: String = "event_msg", payload: [String: Any], at: String = "2026-09-07T09:00:00Z") {
        t.ingest(Data(line(type: type, payload: payload, at: at).utf8))
    }
    private func token(_ t: inout CodexTranscript, input: Int, cached: Int, output: Int, at: String = "2026-09-07T09:00:00Z") {
        ingest(&t, payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":input,"cached_input_tokens":cached,"output_tokens":output]]], at: at)
    }
}
