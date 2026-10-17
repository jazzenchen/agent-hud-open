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

    func testOnlyDesktopRolloutsNavigateToTheirThreadAfterTheCacheReopens() async throws {
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let now = Date(timeIntervalSince1970: 1_789_000_000)
        let origins = [("desktop", "vscode", "Codex Desktop"), ("cli", "cli", "Codex Desktop"),
                       ("exec", "exec", "Codex Desktop"), ("ide", "vscode", "codex_vscode"),
                       ("unknown", "other", "new-client")]
        for (id, source, origin) in origins {
            let rows = [line(type: "session_meta", payload: ["id": id, "cwd": "/same-project", "source": source, "originator": origin], at: now.ISO8601Format()),
                        line(payload: ["type": "task_started", "turn_id": "turn"], at: now.ISO8601Format()),
                        line(payload: ["type": "task_complete", "turn_id": "turn"], at: now.addingTimeInterval(1).ISO8601Format())]
            try Data((rows.joined(separator: "\n") + "\n").utf8).write(to: directory.appendingPathComponent("rollout-\(id).jsonl"))
        }
        let database = directory.appendingPathComponent("ledger.sqlite")
        let initial = CodexTranscriptStore(roots: [directory], ledger: try UsageLedger(url: database))
        let indexed = await initial.index(since: .distantPast)
        XCTAssertEqual(indexed.sessions.first(where: { $0.transcript.id == "desktop" })?.transcript.navigationTarget, .codexThread(id: "desktop"))
        let reopened = CodexTranscriptStore(roots: [directory], ledger: try UsageLedger(url: database))
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: reopened,
            history: QuotaHistoryStore(), clock: { now.addingTimeInterval(2) })
        let report = try await provider.fetchUsage(agents: [], historyHours: 168)
        XCTAssertEqual(report.sessions.count, origins.count)
        XCTAssertEqual(report.completions.count, origins.count)
        for session in report.sessions {
            let expected: SessionNavigationTarget? = session.id == "desktop" ? .codexThread(id: session.id) : nil
            XCTAssertEqual(session.navigationTarget, expected)
            XCTAssertEqual(report.completions.first(where: { $0.sessionID == session.id })?.navigationTarget, expected)
        }
    }

    func testCurrentCodexOriginRoutesSameDirectorySessionsAndAllCachedCompletions() async throws {
        let directory = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let now = Date(timeIntervalSince1970: 1_789_000_000)
        let origins = directory.appendingPathComponent("origins")
        try CodexSessionOrigins.prepare(directory: origins)
        for (id, source, origin) in [("a", "vscode", "Codex Desktop"), ("b", "cli", "codex_cli_rs")] {
            var rows = [line(type: "session_meta", payload: ["id": id, "cwd": "/same-project", "source": source, "originator": origin], at: now.ISO8601Format())]
            for turn in ["first", "second"] {
                rows.append(line(payload: ["type": "task_started", "turn_id": turn], at: now.ISO8601Format()))
                rows.append(line(payload: ["type": "task_complete", "turn_id": turn], at: now.addingTimeInterval(1).ISO8601Format()))
            }
            try Data((rows.joined(separator: "\n") + "\n").utf8).write(to: directory.appendingPathComponent("rollout-\(id).jsonl"))
        }
        let ledger = directory.appendingPathComponent("ledger.sqlite")
        _ = await CodexTranscriptStore(roots: [directory], ledger: try UsageLedger(url: ledger)).index(since: .distantPast)
        let store = CodexTranscriptStore(roots: [directory], ledger: try UsageLedger(url: ledger))
        let liveTargets: [String: SessionNavigationTarget] = ["a": .iTermSession(id: "daemon-pane-a"), "b": .iTermSession(id: "daemon-pane-b")]
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: store,
            history: QuotaHistoryStore(), clock: { now.addingTimeInterval(10) }, sessionOriginsDirectory: origins,
            readTerminalOrigins: { _ in liveTargets })
        XCTAssertTrue(provider.watchedDirectories?.contains(origins) == true)

        func assertTargets(_ report: UsageReport, a: SessionNavigationTarget?, b: SessionNavigationTarget?,
                           file: StaticString = #filePath, line: UInt = #line) {
            XCTAssertEqual(report.sessions.count, 2, file: file, line: line)
            XCTAssertEqual(report.completions.count, 4, file: file, line: line)
            for (id, expected) in [("a", a), ("b", b)] {
                XCTAssertEqual(report.sessions.first(where: { $0.id == id })?.navigationTarget, expected, file: file, line: line)
                XCTAssertTrue(report.completions.filter { $0.sessionID == id }.allSatisfy { $0.navigationTarget == expected }, file: file, line: line)
            }
        }
        func record(_ id: String, host: CodexSessionOrigins.Host, target: String?, at: Date) throws {
            let payload = try JSONSerialization.data(withJSONObject: ["session_id": id, "hook_event_name": "UserPromptSubmit"])
            let environment = target.map { ["TERM_PROGRAM": "iTerm.app", "ITERM_SESSION_ID": $0] } ?? [:]
            try CodexSessionOrigins.record(data: payload, host: host, environment: environment, now: at, directory: origins)
        }

        // A current daemon CLI overrides the rollout's original Desktop identity; cwd never binds two sessions.
        assertTargets(try await provider.fetchUsage(agents: [], historyHours: 168), a: liveTargets["a"], b: liveTargets["b"])
        try record("a", host: .cli, target: "embedded-pane-a", at: now)
        try record("b", host: .cli, target: "embedded-pane-b", at: now)
        assertTargets(try await provider.fetchUsage(agents: [], historyHours: 168),
                      a: .iTermSession(id: "embedded-pane-a"), b: .iTermSession(id: "embedded-pane-b"))
        try record("a", host: .daemon, target: "inherited-wrong-pane", at: now.addingTimeInterval(1))
        assertTargets(try await provider.fetchUsage(agents: [], historyHours: 168), a: liveTargets["a"], b: .iTermSession(id: "embedded-pane-b"))
        try record("a", host: .cli, target: nil, at: now.addingTimeInterval(2))
        assertTargets(try await provider.fetchUsage(agents: [], historyHours: 168), a: nil, b: .iTermSession(id: "embedded-pane-b"))
        try record("a", host: .desktop, target: "inherited-wrong-pane", at: now.addingTimeInterval(3))
        assertTargets(try await provider.fetchUsage(agents: [], historyHours: 168), a: .codexThread(id: "a"), b: .iTermSession(id: "embedded-pane-b"))
        try record("a", host: .daemon, target: nil, at: now.addingTimeInterval(4))
        let moving = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: store,
            history: QuotaHistoryStore(), clock: { now.addingTimeInterval(10) }, sessionOriginsDirectory: origins,
            readTerminalOrigins: { _ in
                let payload = try! JSONSerialization.data(withJSONObject: ["session_id": "a", "hook_event_name": "UserPromptSubmit"])
                try? CodexSessionOrigins.record(data: payload, host: .cli, environment: [:],
                    now: now.addingTimeInterval(5), directory: origins)
                return liveTargets
            })
        assertTargets(try await moving.fetchUsage(agents: [], historyHours: 168), a: nil, b: .iTermSession(id: "embedded-pane-b"))
        XCTAssertEqual(CodexSessionOrigins.read(directory: origins)["a"]?.at, now.addingTimeInterval(5),
                       "a host change during the async daemon lookup is applied to this report")
    }

    func testForkHistoryIsDeduplicatedButInternalModelUsageIsIncluded() {
        var t = CodexTranscript()
        ingest(&t, type: "session_meta", payload: ["id":"child", "timestamp":"2026-09-07T09:00:00Z", "source":["subagent":["thread_spawn":["parent_thread_id":"parent"]]]])
        ingest(&t, type: "session_meta", payload: ["id":"parent", "timestamp":"2026-09-07T07:00:00Z", "source":"cli"])
        token(&t, input: 1000, cached: 800, output: 100, at: "2026-09-07T08:00:00Z")
        token(&t, input: 1400, cached: 1000, output: 150)
        XCTAssertEqual(t.usage.count, 1)
        XCTAssertEqual(t.usage.first?.input, 200)
        XCTAssertEqual(t.usage.first?.output, 50)
        XCTAssertEqual(t.id, "child")
        XCTAssertTrue(t.isSubagent)
        var guardian = CodexTranscript()
        ingest(&guardian, type: "session_meta", payload: ["id":"review", "source":["subagent":["other":"guardian"]]])
        token(&guardian, input: 10000, cached: 0, output: 1000)
        XCTAssertEqual(guardian.usage.first?.input, 10000)
        XCTAssertEqual(guardian.usage.first?.output, 1000)
    }

    func testFirstValidMetadataAndOrdinalBoundaryKeepRestampedParentHistoryInherited() throws {
        var t = CodexTranscript()
        ingest(&t, type: "session_meta", payload: ["cwd": "/invalid"])
        ingest(&t, type: "session_meta", payload: [
            "id": "child", "cwd": "/child", "originator": "Codex Desktop", "subagent_history_start_ordinal": 5,
            "source": ["subagent": ["thread_spawn": ["parent_thread_id": "parent", "agent_path": "/root/audit", "agent_nickname": "Ampere"]]],
        ], ordinal: 0)
        ingest(&t, type: "session_meta", payload: ["id": "parent", "cwd": "/parent", "source": "cli", "timestamp": "2026-09-07T07:00:00Z"], ordinal: 1)
        token(&t, input: 1000, cached: 800, output: 100, ordinal: 2)
        ingest(&t, payload: ["type": "task_started", "turn_id": "parent-turn"], ordinal: 3)
        ingest(&t, type: "compacted", payload: [:], ordinal: 4)
        ingest(&t, payload: ["type": "user_message", "message": "Parent task"], ordinal: 5)
        XCTAssertTrue(t.sessionTurns.isEmpty, "copied events have a new timestamp, so their ordinal defines inheritance")
        XCTAssertTrue(t.usage.isEmpty)
        XCTAssertNil(t.task)
        XCTAssertNil(t.lastActivityAt)
        var reopened = try JSONDecoder().decode(CodexTranscript.self, from: JSONEncoder().encode(t))
        ingest(&reopened, payload: ["type": "task_started", "turn_id": "own-turn"], at: "2026-09-07T09:00:01Z", ordinal: 6)
        token(&reopened, input: 1400, cached: 1000, output: 150, at: "2026-09-07T09:00:02Z", ordinal: 7)
        ingest(&reopened, payload: ["type": "user_message", "message": "Child task"], at: "2026-09-07T09:00:03Z", ordinal: 8)
        XCTAssertEqual(reopened.id, "child")
        XCTAssertEqual(reopened.cwd, "/child")
        XCTAssertEqual(reopened.client, "Desktop")
        XCTAssertEqual(reopened.parentThreadID, "parent")
        XCTAssertEqual(reopened.agentName, "Ampere")
        XCTAssertEqual(reopened.startedAt, ISO8601Fast.parse("2026-09-07T09:00:00Z"))
        XCTAssertTrue(reopened.isSubagent)
        XCTAssertFalse(reopened.isInternal)
        XCTAssertEqual(reopened.task, "Child task")
        XCTAssertEqual(reopened.sessionTurns.map(\.turnID), ["own-turn"])
        XCTAssertEqual(reopened.drainMarks().map(\.kind), [.prompt])
        XCTAssertEqual(reopened.usage.first?.input, 200, "inherited totals still establish the cumulative baseline")
        XCTAssertEqual(reopened.usage.first?.cachedInput, 200)
        XCTAssertEqual(reopened.usage.first?.output, 50)
        XCTAssertNil(reopened.completions)
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
        XCTAssertFalse(t.isLive(now: start.addingTimeInterval(30 + UsageRefresh.abandonedTurnTimeout), modifiedAt: start),
                       "a turn whose last event is this old was abandoned")
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

    func testCompletionUsesTheFinalAnswerAndKeepsItOutOfTheLedger() throws {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start)
        ingest(&value, payload: ["type": "task_started", "turn_id": "first"], at: start.addingTimeInterval(1))
        ingest(&value, payload: ["type": "agent_message", "message": "Checking the files", "channel": "commentary"], at: start.addingTimeInterval(2))
        let final = String(repeating: "First paragraph. ", count: 300) + "\n\nThe final paragraph of this answer."
        ingest(&value, payload: ["type": "task_complete", "turn_id": "first", "last_agent_message": final], at: start.addingTimeInterval(3))
        let completion = try XCTUnwrap(value.completions?.first)
        XCTAssertEqual(completion.message, "The final paragraph of this answer.", "the final paragraph is selected before truncating a long answer")

        ingest(&value, payload: ["type": "task_started", "turn_id": "second"], at: start.addingTimeInterval(4))
        ingest(&value, payload: ["type": "agent_message", "message": "Still checking", "channel": "commentary"], at: start.addingTimeInterval(5))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "second"], at: start.addingTimeInterval(6))
        XCTAssertNil(value.completions?.last?.message, "a completion never substitutes commentary or an earlier answer")

        let encoded = try JSONEncoder().encode(value)
        XCTAssertFalse(String(decoding: encoded, as: UTF8.self).contains("The final paragraph"))
        let restored = try JSONDecoder().decode(CodexTranscript.self, from: encoded)
        XCTAssertEqual(restored.completions?.map(\.id), value.completions?.map(\.id))
        XCTAssertTrue(restored.completions?.allSatisfy { $0.message == nil } == true)
    }

    func testCompletionFallbackRequiresAnExplicitFinalMessageFromItsOwnTurn() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start)
        ingest(&value, payload: ["type": "task_started", "turn_id": "first"], at: start.addingTimeInterval(1))
        ingest(&value, payload: ["type": "agent_message", "turn_id": "first", "message": "First answer", "channel": "final"], at: start.addingTimeInterval(2))
        ingest(&value, payload: ["type": "task_started", "turn_id": "second"], at: start.addingTimeInterval(4))
        ingest(&value, payload: ["type": "agent_message", "turn_id": "missing", "message": "Unrelated answer", "channel": "final"], at: start.addingTimeInterval(5))
        ingest(&value, payload: ["type": "agent_message", "message": "Late earlier answer", "channel": "final"], at: start.addingTimeInterval(3))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "first"], at: start.addingTimeInterval(3))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "second"], at: start.addingTimeInterval(6))
        XCTAssertEqual(value.completions?.map(\.message), ["First answer", nil])

        ingest(&value, payload: ["type": "task_started", "turn_id": "current"], at: start.addingTimeInterval(7))
        let item: [String: Any] = ["type": "AgentMessage", "phase": "final_answer", "content": [["type": "Text", "text": "Current answer\n\nFinal summary"]]]
        ingest(&value, payload: ["type": "item_completed", "turn_id": "current", "item": item], at: start.addingTimeInterval(8))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "current", "last_agent_message": ""], at: start.addingTimeInterval(9))
        XCTAssertEqual(value.completions?.last?.message, "Final summary")

        ingest(&value, payload: ["type": "task_started", "turn_id": "long"], at: start.addingTimeInterval(10))
        let paragraph = "Beginning of the final paragraph. " + String(repeating: "detail ", count: 400)
        ingest(&value, payload: ["type": "agent_message", "turn_id": "long", "message": "Earlier paragraph\n\n" + paragraph, "channel": "final"],
               at: start.addingTimeInterval(11))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "long"], at: start.addingTimeInterval(12))
        XCTAssertEqual(value.completions?.last?.message, String(paragraph.prefix(599)) + "…", "a long final paragraph retains its beginning before clipping")
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

    func testATaskCompleteThatCarriesAnErrorEndsTheTurnWithoutACompletion() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start)
        // The shape Codex writes when a usage limit stops a turn: a terminal task_complete with a structured error.
        ingest(&value, payload: ["type": "task_started", "turn_id": "limited"], at: start.addingTimeInterval(1))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "limited",
                                 "error": ["codex_error_info": "usage_limit_exceeded", "message": "You've hit your usage limit."]],
               at: start.addingTimeInterval(5))
        XCTAssertEqual(value.sessionTurns.map(\.state), [.ended], "a failed turn ended; it did not complete")
        XCTAssertEqual(value.completions?.count ?? 0, 0, "and it earns no completion reminder")
        XCTAssertFalse(value.isLive(now: start.addingTimeInterval(6), modifiedAt: start.addingTimeInterval(5)))

        // A plain string error is a failure too; a null or empty `error` is not one.
        ingest(&value, payload: ["type": "task_started", "turn_id": "text"], at: start.addingTimeInterval(10))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "text", "error": "stream closed"], at: start.addingTimeInterval(12))
        ingest(&value, payload: ["type": "task_started", "turn_id": "null"], at: start.addingTimeInterval(20))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "null", "error": NSNull()], at: start.addingTimeInterval(22))
        ingest(&value, payload: ["type": "task_started", "turn_id": "empty"], at: start.addingTimeInterval(30))
        ingest(&value, payload: ["type": "task_complete", "turn_id": "empty", "error": ""], at: start.addingTimeInterval(32))
        XCTAssertEqual(value.sessionTurns.map(\.state), [.ended, .ended, .completed, .completed])
        XCTAssertEqual(value.completions?.map { $0.completedAt.timeIntervalSince(start) }, [22, 32], "only the turns without an error complete")

        // Replaying the failed line changes nothing.
        ingest(&value, payload: ["type": "task_complete", "turn_id": "limited",
                                 "error": ["codex_error_info": "usage_limit_exceeded", "message": "You've hit your usage limit."]],
               at: start.addingTimeInterval(5))
        XCTAssertEqual(value.sessionTurns.map(\.state), [.ended, .ended, .completed, .completed])
        XCTAssertEqual(value.completions?.count, 2)
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

    func testInheritedTurnsAreSuppressedAndChildTurnsRemainAvailableToTheirDetail() {
        let start = Date(timeIntervalSince1970: 1_800_000_000)
        var value = CodexTranscript()
        ingest(&value, type: "session_meta", payload: ["id": "session", "source": "cli"], at: start.addingTimeInterval(10))
        ingest(&value, payload: ["type": "task_started", "turn_id": "inherited"], at: start.addingTimeInterval(1))
        XCTAssertTrue(value.sessionTurns.isEmpty)
        var child = CodexTranscript()
        ingest(&child, type: "session_meta", payload: ["id": "child", "source": ["subagent": ["thread_spawn": [:]]]], at: start)
        ingest(&child, payload: ["type": "task_started", "turn_id": "child-turn"], at: start.addingTimeInterval(1))
        XCTAssertEqual(child.sessionTurns.map(\.turnID), ["child-turn"])
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

    func testAnArchivedOrDeletedThreadKeepsItsUsageAndLeavesTheSessionList() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let live = dir.appendingPathComponent("sessions/2026/09/09"), archive = dir.appendingPathComponent("archived_sessions")
        for folder in [live, archive] { try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true) }
        let now = Date(timeIntervalSince1970: 1_789_000_000)
        for (id, input) in [("open", 500), ("shelved", 300)] {
            let rows = [line(type: "session_meta", payload: ["id": id, "source": "cli"], at: now.ISO8601Format()),
                        line(payload: ["type": "token_count", "info": ["total_token_usage": ["input_tokens": input, "cached_input_tokens": 0, "output_tokens": 10]]],
                             at: now.ISO8601Format())]
            try (rows.joined(separator: "\n") + "\n").write(to: live.appendingPathComponent("rollout-\(id).jsonl"), atomically: true, encoding: .utf8)
        }
        let ledger = UsageLedger.inMemory()
        let store = CodexTranscriptStore(roots: [dir.appendingPathComponent("sessions"), archive], ledger: ledger)
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: store,
                                          history: QuotaHistoryStore(), clock: { now.addingTimeInterval(10) })
        func tokens() async throws -> Int { try await ledger.buckets(since: .distantPast).reduce(0) { $0 + $1.tokensIn } }
        var report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(Set(report.sessions.map(\.id)), ["open", "shelved"])
        var total = try await tokens()
        XCTAssertEqual(total, 800)

        // Codex archives a thread by moving its rollout.
        try FileManager.default.moveItem(at: live.appendingPathComponent("rollout-shelved.jsonl"), to: archive.appendingPathComponent("rollout-shelved.jsonl"))
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.map(\.id), ["open"], "an archived thread leaves the list")
        total = try await tokens()
        XCTAssertEqual(total, 800, "and its usage still counts, once")

        // A rollout deleted outright leaves the list and keeps its usage too.
        try FileManager.default.removeItem(at: live.appendingPathComponent("rollout-open.jsonl"))
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.map(\.id), [])
        total = try await tokens()
        XCTAssertEqual(total, 800)
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

    func testClientsTheCatalogDoesNotNameAreShownAsWritten() {
        for (origin, source, expected) in [("codex_mcp_server", "mcp", "codex_mcp_server"), ("vibearound", "vscode", "vibearound"),
                                           ("codex_vscode", "vscode", "IDE"), ("Codex Desktop", "vscode", "Desktop")] {
            var t = CodexTranscript()
            ingest(&t, type: "session_meta", payload: ["id":"session", "cwd":"/project", "source":source, "originator":origin])
            XCTAssertEqual(t.client, expected, "\(origin) from \(source)")
        }
        var legacy = CodexTranscript()
        ingest(&legacy, type: "session_meta", payload: ["id":"session", "cwd":"/project", "source":"vscode"])
        XCTAssertEqual(legacy.client, "IDE", "rollouts without an originator keep the extension's name")
        XCTAssertTrue(VendorCatalog.unnamed["Codex client"]?.contains("codex_mcp_server") == true)
    }

    /// Every name Codex gives one of its clients files the session under that client: the desktop app signed in to a work
    /// workspace writes codex_work_desktop, and Codex's own code knows the desktop app as codex_desktop and its CLI as
    /// codex_cli_rs and codex-cli.
    func testEachOriginatorOfOneClientFilesItsSessionsUnderThatClient() {
        let cases: [(origin: String, client: String, name: String)] = [
            ("Codex Desktop", "Desktop", "Codex Desktop"), ("codex_work_desktop", "Desktop", "Codex Desktop"),
            ("codex_desktop", "Desktop", "Codex Desktop"), ("codex_vscode", "IDE", "Codex IDE extension"), ("codex-tui", "CLI", "Codex CLI"),
            ("codex_cli_rs", "CLI", "Codex CLI"), ("codex-cli", "CLI", "Codex CLI"), ("codex_exec", "CLI · exec", "Codex CLI"),
        ]
        for (origin, client, name) in cases {
            var t = CodexTranscript()
            ingest(&t, type: "session_meta", payload: ["id":"session", "cwd":"/project", "source":"vscode", "originator":origin])
            XCTAssertEqual(t.client, client, origin)
            let source = SessionSource(vendor: "Codex", client: t.client)
            XCTAssertEqual(source.name, name, origin)
        }
        XCTAssertEqual(SessionSource(vendor: "Codex", client: "Desktop"), SessionSource(vendor: "Codex", client: VendorCatalog.client("codex_work_desktop", vendor: "Codex")),
                       "one entry in the sessions filter")
    }

    func testWindowNamesComeFromTheCatalogOrTheService() throws {
        L10n.setLanguage(.en)
        defer { L10n.setLanguage(.system) }
        let limits = try decode(#"{"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":3,"windowDurationMins":300}},"base_model_inference":{"limitName":"gpt-reserve","primary":{"usedPercent":0,"windowDurationMins":10080}},"codex_next":{"limitName":"Next Model","primary":{"usedPercent":5,"windowDurationMins":10080}}}}"#)
        let labels = Dictionary(uniqueKeysWithValues: limits.rows.map { ($0.id, $0.label) })
        XCTAssertEqual(labels["codex"], "5h limit")
        XCTAssertEqual(labels["codex:base_model_inference:primary"], "Luna Reserve · Weekly limit", "the name OpenAI's own client shows")
        XCTAssertEqual(labels["codex:codex_next:primary"], "Next Model · Weekly limit", "a window the catalog does not name keeps the service's name")
        let short = Dictionary(uniqueKeysWithValues: limits.rows.map { ($0.id, $0.descriptor.shortName) })
        XCTAssertEqual(short["codex:base_model_inference:primary"], "Reserve", "the catalog's word")
        XCTAssertEqual(short["codex:codex_next:primary"], "Model", "the last word of a name the catalog does not know")
    }

    func testOnlyTheCodexBucketCoversEveryModel() throws {
        let limits = try decode(#"{"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":3,"windowDurationMins":300},"secondary":{"usedPercent":9,"windowDurationMins":10080}},"base_model_inference":{"limitName":"gpt-reserve","primary":{"usedPercent":0,"windowDurationMins":10080}}}}"#)
        XCTAssertEqual(limits.rows.map(\.id), ["codex", "codex:codex:secondary", "codex:base_model_inference:primary"])
        XCTAssertEqual(limits.rows.map(\.descriptor.allModels), [true, true, false])
        XCTAssertEqual(limits.rows(home: "").map(\.descriptor.allModels), [true, true, false], "an account's rows keep the flag")
    }

    func testLocatorWorksWithoutDesktopOrWithoutCLI() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let apps = dir.appendingPathComponent("Applications")
        let cli = dir.appendingPathComponent(".bun/bin/codex")
        try FileManager.default.createDirectory(at: cli.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: cli, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: cli.path)
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: "", registered: []), cli)
        try FileManager.default.removeItem(at: cli)
        let desktop = apps.appendingPathComponent("Codex.app/Contents/Resources/codex")
        try FileManager.default.createDirectory(at: desktop.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: desktop, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: desktop.path)
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: "", registered: []), desktop)
    }

    func testLocatorPrefersTheDesktopEngineWhereCurrentBuildsKeepIt() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let apps = dir.appendingPathComponent("Applications")
        let engine = apps.appendingPathComponent("ChatGPT.app/Contents/Resources/codex-cli/bin/codex")
        let cli = dir.appendingPathComponent(".bun/bin/codex")
        for file in [engine, cli] {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try "#!/bin/sh\n".write(to: file, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        }
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: "", registered: []), engine,
                       "an installed CLI can be older than the app's engine and leave out accountId")
    }

    func testLocatorFindsTheAppUnderItsBundleIDWhateverItIsCalled() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let renamed = dir.appendingPathComponent("Elsewhere/Renamed.app")
        let engine = renamed.appendingPathComponent("Contents/Resources/codex")
        try FileManager.default.createDirectory(at: engine.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: engine, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: engine.path)
        let cli = dir.appendingPathComponent(".bun/bin/codex")
        try FileManager.default.createDirectory(at: cli.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "#!/bin/sh\n".write(to: cli, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: cli.path)
        let apps = dir.appendingPathComponent("Applications")
        XCTAssertEqual(CodexLocator.find(home: dir, applications: apps, path: "", registered: [renamed]), engine,
                       "the app's own engine is preferred to the CLI even under a name the locator does not know")
    }

    func testASessionNamesTheRolloutsOfItsSpawnedAgentsAndGuardians() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let rollouts: [String: [String: Any]] = [
            "rollout-parent.jsonl": ["id": "parent", "source": "cli"],
            "rollout-child.jsonl": ["id": "child", "cwd": "/child", "source": ["subagent": ["thread_spawn": ["parent_thread_id": "parent", "agent_path": "/root/child"]]]],
            "rollout-grandchild.jsonl": ["id": "grandchild", "source": ["subagent": ["thread_spawn": ["parent_thread_id": "child", "agent_nickname": "Curie"]]]],
            "rollout-guardian.jsonl": ["id": "guardian", "parent_thread_id": "child", "source": ["subagent": ["other": "guardian"]]],
            "rollout-other.jsonl": ["id": "other", "source": "cli"],
        ]
        for (name, meta) in rollouts {
            var rows = [line(type: "session_meta", payload: meta)]
            if meta["id"] as? String == "child" {
                rows += [line(type: "session_meta", payload: ["id": "parent", "source": "cli", "cwd": "/parent"]),
                         line(type: "turn_context", payload: ["model": "gpt-6-astra"]),
                         line(payload: ["type": "user_message", "message": "Review the parser"]),
                         line(payload: ["type": "task_started", "turn_id": "child-turn"]),
                         line(payload: ["type": "token_count", "info": ["total_token_usage": ["input_tokens": 100, "cached_input_tokens": 20, "output_tokens": 10]]])]
            }
            try (rows.joined(separator: "\n") + "\n").write(to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
        try FileManager.default.setAttributes([.modificationDate: Date().addingTimeInterval(1)], ofItemAtPath: dir.appendingPathComponent("rollout-child.jsonl").path)
        let now = self.now
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: CodexTranscriptStore(roots: [dir]), history: QuotaHistoryStore(), clock: { now })
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 48)
        let sessions = Dictionary(uniqueKeysWithValues: report.sessions.map { ($0.id, $0) })
        XCTAssertEqual(Set(sessions.keys), ["parent", "other"], "sub-agents are not sessions of their own")
        XCTAssertEqual(sessions["parent"]?.subagentTranscripts?.map { URL(fileURLWithPath: $0).lastPathComponent },
                       ["rollout-child.jsonl", "rollout-grandchild.jsonl", "rollout-guardian.jsonl"], "a guardian of the spawned agent is below the session too")
        XCTAssertNil(sessions["other"]?.subagentTranscripts)
        let child = try XCTUnwrap(sessions["parent"]?.subagentSessions?.first)
        XCTAssertEqual(sessions["parent"]?.transcriptPath.map { URL(fileURLWithPath: $0).lastPathComponent }, "rollout-parent.jsonl",
                       "a newer child file never replaces the root's transcript")
        XCTAssertEqual(sessions["parent"]?.subagentSessions?.map(\.id), ["child"])
        XCTAssertEqual(child.id, "child")
        XCTAssertEqual(child.agentName, "child")
        XCTAssertEqual(child.task, "Review the parser")
        XCTAssertEqual(child.workingDirectory, "/child")
        XCTAssertEqual(child.tokensIn, 80)
        XCTAssertEqual(child.cacheReadTokens, 20)
        XCTAssertEqual(child.tokensOut, 10)
        XCTAssertEqual(child.transcriptPath.map { URL(fileURLWithPath: $0).lastPathComponent }, "rollout-child.jsonl")
        XCTAssertEqual(child.subagentSessions?.map(\.id), ["grandchild"], "internal guardians do not become visible agents")
        XCTAssertEqual(child.subagentSessions?.first?.agentName, "Curie")
        XCTAssertEqual(report.turns.map(\.sessionID), ["child"])
    }

    func testAMonthOfTokensListsTheSessionsOfTheLastSevenDaysWithTheirWholeTrees() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let now = Date()
        func rollout(_ id: String, source: Any, model: String, daysAgo: Double) throws {
            let at = now.addingTimeInterval(-daysAgo * 86400), file = dir.appendingPathComponent("rollout-\(id).jsonl")
            let rows = [line(type: "session_meta", payload: ["id": id, "cwd": "/\(id)", "source": source], at: at.ISO8601Format()),
                        line(payload: ["type": "task_started", "turn_id": "\(id)-turn"], at: at.ISO8601Format()),
                        line(type: "turn_context", payload: ["model": model], at: at.ISO8601Format()),
                        line(payload: ["type": "token_count", "info": ["total_token_usage": ["input_tokens": 100, "output_tokens": 10]]],
                             at: at.ISO8601Format()),
                        line(payload: ["type": "task_complete", "turn_id": "\(id)-turn"], at: at.addingTimeInterval(1).ISO8601Format())]
            try (rows.joined(separator: "\n") + "\n").write(to: file, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.modificationDate: at.addingTimeInterval(1)], ofItemAtPath: file.path)
        }
        try rollout("parent", source: "cli", model: "gpt-6-astra", daysAgo: 1)
        // A sub-agent the recent session started weeks ago, and a session nobody has touched since then.
        try rollout("child", source: ["subagent": ["thread_spawn": ["parent_thread_id": "parent"]]], model: "gpt-6-astra", daysAgo: 20)
        try rollout("old", source: "cli", model: "gpt-6-sol", daysAgo: 20)
        let ledger = UsageLedger.inMemory()
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") },
            transcripts: CodexTranscriptStore(roots: [dir], ledger: ledger), history: QuotaHistoryStore(), clock: { now })
        let report = try await provider.fetchUsage(agents: [], historyHours: 31 * 24 + 25)
        XCTAssertEqual(report.sessions.map(\.id), ["parent"])
        let parent = try XCTUnwrap(report.sessions.first)
        XCTAssertEqual(parent.subagentSessions?.map(\.id), ["child"], "a listed session keeps its whole tree")
        XCTAssertEqual(parent.subagentTranscripts?.map { URL(fileURLWithPath: $0).lastPathComponent }, ["rollout-child.jsonl"],
                       "its breakdown still takes the older sub-agent's rollout")
        XCTAssertEqual(Set(report.turns.map(\.sessionID)), ["parent", "child"])
        XCTAssertEqual(report.completions.filter { $0.sessionID != "child" }.map(\.sessionID), ["parent"])
        XCTAssertEqual(Set(report.consumers.map(\.id)), ["codex-model:gpt-6-astra", "codex-model:gpt-6-sol"])
        let recorded = try await ledger.buckets(since: now.addingTimeInterval(-31 * 86400), source: "codex")
        XCTAssertEqual(recorded.reduce(0) { $0 + $1.tokensIn }, 300, "every rollout of the month records its tokens")
    }

    func testQuotaFailureKeepsLocalSessions() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let contents = line(type: "session_meta", payload: ["id":"cli", "source":"cli", "cwd":"/project"]) + "\n" + line(payload: ["type":"task_started"]) + "\n"
        try contents.write(to: dir.appendingPathComponent("rollout-cli.jsonl"), atomically: true, encoding: .utf8)
        let now = self.now
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") }, transcripts: CodexTranscriptStore(roots: [dir]), history: QuotaHistoryStore(), clock: { now })
        let known = AgentDescriptor(id: "codex", vendor: "Codex", model: "5h", source: L10n.sourceCodexAppServer, enabled: true)
        let report = try await provider.fetchAccountAndLocalUsage(agents: [known], historyHours: 48)
        XCTAssertEqual(report.sessions.first?.client, "CLI")
        let session = try XCTUnwrap(report.sessions.first)
        XCTAssertTrue(report.consumerIdsByQuota["codex"]?.contains(session.agentId) == true)
        XCTAssertNil(report.sessions.first?.pctOfWindow)
        XCTAssertTrue(report.snapshots.isEmpty)
        XCTAssertEqual(report.sourceNotices["Codex"], "signed out")
        XCTAssertNil(report.codexResetCredits)
    }

    func testCompletionsUseTheCurrentConversationTitleWithoutChangingIdentity() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let index = dir.appendingPathComponent("session_index.jsonl")
        let contents = [
            line(type: "session_meta", payload: ["id": "session", "source": "cli", "cwd": "/project"]),
            line(payload: ["type": "user_message", "message": "First question"]),
            line(payload: ["type": "task_started", "turn_id": "turn"]),
            line(payload: ["type": "task_complete", "turn_id": "turn", "last_agent_message": "Final answer"]),
        ].joined(separator: "\n") + "\n"
        try contents.write(to: dir.appendingPathComponent("rollout-session.jsonl"), atomically: true, encoding: .utf8)
        let now = self.now
        let provider = CodexUsageProvider(readLimits: { throw UsageProviderError("signed out") },
            transcripts: CodexTranscriptStore(roots: [dir], indexURL: index), history: QuotaHistoryStore(), clock: { now })
        let fallback = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(fallback.completions.first?.task, "First question")

        try #"{"id":"session","thread_name":"Automatic update checks"}"#.write(to: index, atomically: true, encoding: .utf8)
        let named = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(named.sessions.first?.task, "Automatic update checks")
        XCTAssertEqual(named.completions.first?.task, named.sessions.first?.task)
        XCTAssertEqual(named.completions.first?.message, "Final answer")
        XCTAssertEqual(named.completions.first?.id, fallback.completions.first?.id, "a conversation rename does not produce a new completion")

        try #"{"id":"session","thread_name":"A renamed conversation"}"#.write(to: index, atomically: true, encoding: .utf8)
        let renamed = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(renamed.completions.first?.task, "A renamed conversation")
        XCTAssertEqual(renamed.completions.first?.id, named.completions.first?.id)
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
        XCTAssertEqual(report.observation(accountID: account.id)?.quotaWindowIDs,
                       Set(["codex", "codex:spark:primary", "codex:unknown:primary"].map(account.windowID)),
                       "The rate-limit response explicitly supplies its complete inventory, including a window without a known cadence")
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
    private func line(type: String = "event_msg", payload: [String: Any], at: String = "2026-09-07T09:00:00Z", ordinal: Int? = nil) -> String {
        var object: [String: Any] = ["type": type, "timestamp": at, "payload": payload]
        object["ordinal"] = ordinal
        return String(decoding: try! JSONSerialization.data(withJSONObject: object, options: .sortedKeys), as: UTF8.self)
    }
    private func ingest(_ t: inout CodexTranscript, type: String = "event_msg", payload: [String: Any], at: String = "2026-09-07T09:00:00Z", ordinal: Int? = nil) {
        t.ingest(Data(line(type: type, payload: payload, at: at, ordinal: ordinal).utf8))
    }
    private func ingest(_ t: inout CodexTranscript, type: String = "event_msg", payload: [String: Any], at date: Date) {
        ingest(&t, type: type, payload: payload, at: date.ISO8601Format())
    }
    private func token(_ t: inout CodexTranscript, input: Int, cached: Int, output: Int, at: String = "2026-09-07T09:00:00Z", ordinal: Int? = nil) {
        ingest(&t, payload: ["type":"token_count", "info":["total_token_usage":["input_tokens":input,"cached_input_tokens":cached,"output_tokens":output]]], at: at, ordinal: ordinal)
    }
}
