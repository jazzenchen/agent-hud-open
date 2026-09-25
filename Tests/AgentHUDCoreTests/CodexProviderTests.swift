import AgentHUDSupport
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

    func testCollectorPathUpdatesCodexIndexBeforeItsOwnFileWatchDelivers() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("codex-paths-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appendingPathComponent("rollout-test.jsonl")
        let store = CodexTranscriptStore(roots: [directory], watchesChanges: true)
        let initial = await store.index(since: .distantPast)
        XCTAssertTrue(initial.sessions.isEmpty)

        let meta = #"{"timestamp":"2026-09-25T02:00:00Z","type":"session_meta","payload":{"id":"s","cwd":"/project","source":"cli"}}"#
        let started = #"{"timestamp":"2026-09-25T02:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#
        let completed = #"{"timestamp":"2026-09-25T02:00:02Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}"#
        try Data((meta + "\n" + started + "\n").utf8).write(to: file)
        await store.fileChanges([directory.path])
        let running = await store.index(since: .distantPast)
        XCTAssertEqual(running.sessions.first?.transcript.sessionTurns.last?.state, .running,
                       "a directory event from the outer collector discovers a new rollout immediately")

        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((completed + "\n").utf8))
        try handle.close()
        await store.fileChanges([file.path])
        let done = await store.index(since: .distantPast)
        XCTAssertEqual(done.sessions.first?.transcript.sessionTurns.last?.state, .completed,
                       "the collector's path reaches the index even before its own watcher reports the append")
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

    func testCodexRetainsEachSuccessfulTurnAndIgnoresCancellationTimeoutAndInheritedHistory() throws {
        let start = Date(timeIntervalSince1970: 1_788_850_000)
        var t = CodexTranscript()
        ingest(&t, type: "session_meta", payload: ["id": "s", "source": "cli"], at: start)
        ingest(&t, payload: ["type": "task_complete", "turn_id": "inherited"], at: start.addingTimeInterval(-10))
        for (index, kind) in ["task_complete", "turn_aborted", "task_complete"].enumerated() {
            let at = start.addingTimeInterval(Double(index * 10))
            ingest(&t, payload: ["type": "task_started", "turn_id": "t\(index)"], at: at)
            ingest(&t, payload: ["type": kind, "turn_id": "t\(index)"], at: at.addingTimeInterval(3))
        }
        XCTAssertEqual(t.completions?.count, 2)
        XCTAssertEqual(t.completions?.map { $0.completedAt.timeIntervalSince($0.startedAt!) }, [3, 3])
        ingest(&t, payload: ["type": "task_complete", "turn_id": "t2"], at: start.addingTimeInterval(23))
        XCTAssertEqual(t.completions?.count, 2, "replayed line has the same event ID")
        ingest(&t, payload: ["type": "task_started", "turn_id": "stale"], at: start.addingTimeInterval(30))
        XCTAssertTrue(t.isLive(now: start.addingTimeInterval(200), modifiedAt: start), "a quiet rollout does not end an open turn")
        XCTAssertFalse(t.isLive(now: start.addingTimeInterval(UsageRefresh.abandonedTurnTimeout + 1), modifiedAt: start),
                       "a turn quiet this long was abandoned")
        XCTAssertEqual(t.completions?.count, 2, "inactivity produces no completion")
        let restored = try JSONDecoder().decode(CodexTranscript.self, from: JSONEncoder().encode(t))
        XCTAssertEqual(restored.completions, t.completions)
        var oldCache = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(t)) as? [String: Any])
        oldCache.removeValue(forKey: "completions"); oldCache.removeValue(forKey: "turnStartedAt"); oldCache.removeValue(forKey: "turnID")
        XCTAssertNil(try JSONDecoder().decode(CodexTranscript.self, from: JSONSerialization.data(withJSONObject: oldCache)).completions)
    }

    func testReadsReasoningCacheWritesWindowsTurnStartsAndCompactions() {
        var transcript = CodexTranscript()
        let lines = [
            #"{"timestamp":"2026-09-07T06:00:00.000Z","type":"session_meta","payload":{"id":"c-1","timestamp":"2026-09-07T06:00:00.000Z","cwd":"/p","source":"cli"}}"#,
            #"{"timestamp":"2026-09-07T06:00:01.000Z","type":"turn_context","payload":{"model":"gpt-6-astra"}}"#,
            #"{"timestamp":"2026-09-07T06:00:02.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1","model_context_window":258400}}"#,
            #"{"timestamp":"2026-09-07T06:00:10.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":30000,"cached_input_tokens":20000,"cache_write_input_tokens":4000,"output_tokens":500,"reasoning_output_tokens":200},"last_token_usage":{"input_tokens":30000,"cached_input_tokens":20000,"cache_write_input_tokens":4000,"output_tokens":500,"reasoning_output_tokens":200},"model_context_window":258400}}}"#,
            #"{"timestamp":"2026-09-07T06:00:20.000Z","type":"compacted","payload":{"message":"","replacement_history":[]}}"#,
            #"{"timestamp":"2026-09-07T06:00:30.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":42000,"cached_input_tokens":28000,"cache_write_input_tokens":4000,"output_tokens":800,"reasoning_output_tokens":260},"model_context_window":258400}}}"#,
        ]
        for line in lines { transcript.ingest(Data(line.utf8)) }
        let events = transcript.drainUsage()
        XCTAssertEqual(events.map(\.tokensIn), [10_000, 4_000], "cached input is counted apart; cache writes stay in input")
        XCTAssertEqual(events.map(\.cacheReadTokens), [20_000, 8_000])
        XCTAssertEqual(events.map(\.cacheWriteTokens), [4_000, 0])
        XCTAssertEqual(events.map(\.reasoningTokens), [200, 60])
        XCTAssertEqual(events.map(\.tokensOut), [500, 300])
        XCTAssertEqual(events.map(\.contextWindow), [258_400, 258_400])
        XCTAssertEqual(transcript.drainMarks().map(\.kind), [.prompt, .compaction])
    }

    func testSubagentsNeverProduceUserFacingCompletions() {
        let start = Date(timeIntervalSince1970: 1_788_850_000)
        var t = CodexTranscript()
        ingest(&t, type: "session_meta", payload: ["id": "child", "source": ["subagent": ["other": "guardian"]]], at: start)
        ingest(&t, payload: ["type": "task_complete"], at: start.addingTimeInterval(1))
        XCTAssertNil(t.completions)
    }

    func testLateCompletionDoesNotEndTheNextTurn() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start)
        ingest(&value, payload: ["type": "task_started", "turn_id": "first"], at: start.addingTimeInterval(1))
        ingest(&value, payload: ["type": "task_started", "turn_id": "second"], at: start.addingTimeInterval(10))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "first"], at: start.addingTimeInterval(5))
        XCTAssertEqual(value.sessionTurns.map(\.state), [.completed, .running])
        XCTAssertEqual(value.sessionTurns.last?.turnID, "second")
        XCTAssertTrue(value.isLive(now: start.addingTimeInterval(11), modifiedAt: start.addingTimeInterval(10)))
        ingest(&value, payload: ["type": "task_complete"], at: start.addingTimeInterval(12))
        XCTAssertEqual(value.sessionTurns.last?.state, .running, "An unassociated completion cannot finish an identified turn")
        ingest(&value, payload: ["type": "turn_aborted", "turn_id": "second"], at: start.addingTimeInterval(15))
        XCTAssertEqual(value.sessionTurns.last?.state, .ended)
        XCTAssertFalse(value.isLive(now: start.addingTimeInterval(16), modifiedAt: start.addingTimeInterval(15)))
    }

    func testOnlyExplicitIdentifiedTurnsAreReportedAndRetainSourceTime() throws {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start)
        ingest(&value, payload: ["type": "user_message", "message": "example"], at: start.addingTimeInterval(1))
        XCTAssertTrue(value.sessionTurns.isEmpty)
        ingest(&value, payload: ["type": "task_started"], at: start.addingTimeInterval(2))
        XCTAssertTrue(value.sessionTurns.isEmpty)
        ingest(&value, payload: ["type": "task_started", "turn_id": "identified"], at: start.addingTimeInterval(3))
        ingest(&value, payload: ["type": "agent_message", "message": "working"], at: start.addingTimeInterval(8))
        let before = try XCTUnwrap(value.sessionTurns.last)
        XCTAssertEqual(before.observedAtMs, RecordCoding.milliseconds(start.addingTimeInterval(8)))
        XCTAssertEqual(before.message, "working", "the visible answer travels with the turn")
        let restored = try JSONDecoder().decode(CodexTranscript.self, from: JSONEncoder().encode(value))
        XCTAssertEqual(restored.sessionTurns.map(\.turnID), value.sessionTurns.map(\.turnID))
        XCTAssertEqual(restored.sessionTurns.map(\.state), value.sessionTurns.map(\.state))
        XCTAssertNil(restored.sessionTurns.last?.message, "what the agent said is not kept in the ledger")
        ingest(&value, payload: ["type": "task_complete", "turn_id": "identified"], at: start.addingTimeInterval(12))
        let ended = try XCTUnwrap(value.sessionTurns.last)
        XCTAssertEqual(ended.id, before.id)
        XCTAssertEqual(ended.message, "working", "a finished turn still shows what it said")
        ingest(&value, payload: ["type": "task_complete", "turn_id": "identified"], at: start.addingTimeInterval(14))
        XCTAssertEqual(value.sessionTurns.last, ended)
    }

    func testInheritedAndSubagentTurnsAreNotReported() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start.addingTimeInterval(10))
        ingest(&value, payload: ["type": "task_started", "turn_id": "inherited"], at: start.addingTimeInterval(1))
        XCTAssertTrue(value.sessionTurns.isEmpty)
        var child = CodexTranscript()
        ingest(&child, type: "session_meta", payload: ["id": "child", "source": ["subagent": ["thread_spawn": [:]]]], at: start)
        ingest(&child, payload: ["type": "task_started", "turn_id": "child-turn"], at: start.addingTimeInterval(1))
        XCTAssertTrue(child.sessionTurns.isEmpty)
    }

    func testTitleIsTheFirstOwnPromptInCurrentAndLegacyRollouts() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        let prompt = { (parts: [[String: Any]]) -> [String: Any] in
            ["type": "item_completed", "turn_id": "t", "item": ["type": "UserMessage", "id": "u", "content": parts]]
        }
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "vscode"], at: start)
        ingest(&value, payload: prompt([["type": "text", "text": "Inherited prompt"]]), at: start.addingTimeInterval(-5))
        ingest(&value, payload: ["type": "item_completed", "item": ["type": "AgentMessage", "content": [["type": "text", "text": "Reply"]]]], at: start)
        ingest(&value, payload: prompt([["type": "local_image", "path": "/tmp/shot.png"], ["type": "text", "text": "<environment_context>"]]),
               at: start.addingTimeInterval(1))
        XCTAssertNil(value.task)
        ingest(&value, payload: prompt([["type": "image", "image_url": "data:"], ["type": "text", "text": "  Fix the sync bug\nwith details", "text_elements": []]]),
               at: start.addingTimeInterval(2))
        ingest(&value, payload: prompt([["type": "text", "text": "Second prompt"]]), at: start.addingTimeInterval(3))
        XCTAssertEqual(value.task, "Fix the sync bug")
        var legacy = CodexTranscript()
        ingest(&legacy, type: "session_meta", payload: ["id": "legacy", "source": "cli"], at: start)
        ingest(&legacy, payload: ["type": "user_message", "message": String(repeating: "a", count: 80)], at: start.addingTimeInterval(1))
        XCTAssertEqual(legacy.task, String(repeating: "a", count: 59) + "…")
    }

    func testIncrementalStoreHandlesPartialLineAndArchiveDuplicate() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("rollout-test.jsonl")
        let meta = line(type: "session_meta", payload: ["id":"same", "source":"cli"])
        let usage = line(payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":500, "cached_input_tokens":200, "output_tokens":50]]])
        try (meta + "\n" + String(usage.prefix(70))).write(to: file, atomically: true, encoding: .utf8)
        let ledgerURL = dir.appendingPathComponent("ledger/usage-ledger.sqlite")
        let store = CodexTranscriptStore(roots: [dir], ledger: try UsageLedger(url: ledgerURL))
        let first = await store.index(since: .distantPast)
        XCTAssertEqual(first.sessions.count, 1)
        XCTAssertEqual(first.sessions[0].transcript.inputTokens, 0, "a partial line is not read yet")
        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd(); try handle.write(contentsOf: Data((String(usage.dropFirst(70)) + "\n").utf8)); try handle.close()
        let second = await store.index(since: .distantPast)
        XCTAssertEqual(second.sessions[0].transcript.inputTokens, 300)
        XCTAssertEqual(second.sessions[0].transcript.cachedInputTokens, 200)
        let recorded = await store.usage(since: .distantPast)
        XCTAssertEqual(recorded.map(\.tokensIn), [300])
        XCTAssertEqual(recorded.map(\.cacheReadTokens), [200])
        try FileManager.default.copyItem(at: file, to: dir.appendingPathComponent("rollout-archived.jsonl"))
        let reopened = try UsageLedger(url: ledgerURL)
        let restored = CodexTranscriptStore(roots: [dir], ledger: reopened)
        let third = await restored.index(since: .distantPast)
        XCTAssertEqual(third.sessions.count, 1)
        XCTAssertEqual(third.sessions[0].transcript.inputTokens, 300)
        let counted = try await reopened.buckets(since: .distantPast)
        XCTAssertEqual(counted.map(\.tokensIn), [300], "an archived copy of the same session counts once")
        try FileManager.default.removeItem(at: file)
        _ = await restored.index(since: .distantPast)
        let moved = try await reopened.buckets(since: .distantPast)
        XCTAssertEqual(moved.map(\.tokensIn), [300], "the remaining copy takes over when the original is gone")
    }

    func testRestartDoesNotReadAnUnchangedRolloutAgain() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("rollout-restart.jsonl"), ledgerURL = dir.appendingPathComponent("ledger/usage-ledger.sqlite")
        let meta = line(type: "session_meta", payload: ["id":"restart", "source":"cli"])
        let usage = { (input: Int) in self.line(payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":input, "cached_input_tokens":0, "output_tokens":5]]]) }
        try (meta + "\n" + usage(100) + "\n").write(to: file, atomically: true, encoding: .utf8)
        let modified = Date(timeIntervalSince1970: 1_788_800_000.123456)
        try FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: file.path)
        _ = await CodexTranscriptStore(roots: [dir], ledger: try UsageLedger(url: ledgerURL)).index(since: .distantPast)
        // Same size and modification time, different bytes: only a re-read could notice.
        try (meta + "\n" + usage(900) + "\n").write(to: file, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: file.path)
        let restarted = await CodexTranscriptStore(roots: [dir], ledger: try UsageLedger(url: ledgerURL)).index(since: .distantPast)
        XCTAssertEqual(restarted.sessions.first?.transcript.inputTokens, 100)
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
            elif request['method']=='account/read':
                print(json.dumps({'id':3,'result':{'account':{'type':'chatgpt','email':'Dev@Example.com','planType':'prolite'},'requiresOpenaiAuth':True}}),flush=True)
        """
        let exe = dir.appendingPathComponent("codex")
        try script.write(to: exe, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: exe.path)
        let result = try await CodexAppServerClient(executable: exe, dataDirectory: dir, timeout: 5).fetch()
        XCTAssertEqual(result.rows.count, 3)
        XCTAssertEqual(result.rateLimitResetCredits?.availableCount, 3)
        XCTAssertEqual(result.account?.email, "Dev@Example.com")
        XCTAssertEqual(try String(contentsOf: dir.appendingPathComponent("methods"), encoding: .utf8),
                       "initialize\ninitialized\naccount/rateLimits/read\naccount/read\n")
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

    func testASessionNamesTheRolloutsOfItsSpawnedAgentsAndGuardians() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let rollouts: [String: [String: Any]] = [
            "rollout-parent.jsonl": ["id": "parent", "source": "cli"],
            "rollout-child.jsonl": ["id": "child", "source": ["subagent": ["thread_spawn": ["parent_thread_id": "parent"]]]],
            "rollout-guardian.jsonl": ["id": "guardian", "parent_thread_id": "child", "source": ["subagent": ["other": "guardian"]]],
            "rollout-other.jsonl": ["id": "other", "source": "cli"],
        ]
        for (name, meta) in rollouts {
            try (line(type: "session_meta", payload: meta) + "\n").write(to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: CodexTranscriptStore(roots: [dir]), history: QuotaHistoryStore())
        let report = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        let sessions = Dictionary(uniqueKeysWithValues: report.sessions.map { ($0.id, $0) })
        XCTAssertEqual(Set(sessions.keys), ["parent", "other"], "sub-agents are not sessions of their own")
        XCTAssertEqual(sessions["parent"]?.subagentTranscripts?.map { URL(fileURLWithPath: $0).lastPathComponent },
                       ["rollout-child.jsonl", "rollout-guardian.jsonl"], "a guardian of the spawned agent is below the session too")
        XCTAssertNil(sessions["other"]?.subagentTranscripts)
    }

    func testQuotaFailureKeepsLocalSessions() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let contents = line(type: "session_meta", payload: ["id":"cli", "source":"cli", "cwd":"/project"]) + "\n" + line(payload: ["type":"task_started"]) + "\n"
        try contents.write(to: dir.appendingPathComponent("rollout-cli.jsonl"), atomically: true, encoding: .utf8)
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: CodexTranscriptStore(roots: [dir]), history: QuotaHistoryStore())
        let report = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        XCTAssertEqual(report.sessions.first?.client, "CLI")
        let session = try XCTUnwrap(report.sessions.first)
        XCTAssertTrue(report.consumerIdsByQuota["codex"]?.contains(session.agentId) == true)
        XCTAssertNil(report.sessions.first?.pctOfWindow)
        XCTAssertTrue(report.snapshots.isEmpty)
        XCTAssertEqual(report.sourceNotices["Codex"], "signed out")
        XCTAssertNil(report.codexResetCredits)
    }

    func testForecastUsesEachServicesWindowPeriodAndRecentPace() async throws {
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
        let account = ProviderAccount.unresolved(provider: "Codex", home: "")
        let history = QuotaHistoryStore()
        await history.append([
            QuotaSample(agentId: account.windowID("codex"), timestamp: now.addingTimeInterval(-5 * 86400), remainingPct: 100),
            QuotaSample(agentId: account.windowID("codex"), timestamp: now.addingTimeInterval(-30 * 3600), remainingPct: 70),
            QuotaSample(agentId: account.windowID("codex:spark:primary"), timestamp: now.addingTimeInterval(-3 * 3600), remainingPct: 100),
            QuotaSample(agentId: account.windowID("codex:spark:primary"), timestamp: now.addingTimeInterval(-3600), remainingPct: 60),
            QuotaSample(agentId: account.windowID("codex:unknown:primary"), timestamp: now.addingTimeInterval(-3600), remainingPct: 80)
        ], now: now)
        let provider = CodexUsageProvider(readLimits: { limits }, transcripts: CodexTranscriptStore(roots: [dir]),
                                          history: history, clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.snapshot(for: account.windowID("codex"))?.windowDuration, 7 * 86400)
        XCTAssertEqual(report.snapshot(for: account.windowID("codex:spark:primary"))?.windowDuration, 5 * 3600)
        let weekly = try XCTUnwrap(report.insightsByAgent[account.windowID("codex")])
        XCTAssertEqual(try XCTUnwrap(weekly.burnRatePctPerHour), 10.0 / 30, accuracy: 1e-9,
                       "the statistics range must not truncate the readings behind the last day")
        XCTAssertEqual(try XCTUnwrap(weekly.timeToExhaust), 180 * 3600, accuracy: 1e-6)
        let session = try XCTUnwrap(report.insightsByAgent[account.windowID("codex:spark:primary")])
        XCTAssertEqual(try XCTUnwrap(session.burnRatePctPerHour), 20, accuracy: 1e-9,
                       "the quiet first two hours do not dilute the last one")
        XCTAssertEqual(try XCTUnwrap(session.timeToExhaust), 2 * 3600, accuracy: 1e-6)
        XCTAssertNil(report.insightsByAgent[account.windowID("codex:unknown:primary")]?.burnRatePctPerHour)
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
    private func ingest(_ t: inout CodexTranscript, type: String = "event_msg", payload: [String: Any], at date: Date) {
        ingest(&t, type: type, payload: payload, at: date.ISO8601Format())
    }
    private func token(_ t: inout CodexTranscript, input: Int, cached: Int, output: Int, at: String = "2026-09-07T09:00:00Z") {
        ingest(&t, payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":input,"cached_input_tokens":cached,"output_tokens":output]]], at: at)
    }
}
