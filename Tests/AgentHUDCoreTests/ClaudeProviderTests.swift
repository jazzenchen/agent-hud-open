import AgentHUDSupport
import XCTest
@testable import AgentHUDCore

final class ClaudeUsageParseTests: XCTestCase {
    func testParsesWindows() throws {
        let json = #"{"five_hour":{"utilization":28,"resets_at":"2026-09-07T10:14:00.000Z"},"seven_day":{"utilization":39.5,"resets_at":"2026-09-10T00:00:00Z"},"seven_day_opus":{"utilization":61.2,"resets_at":null}}"#
        let usage = try ClaudeUsage.parse(Data(json.utf8))
        XCTAssertEqual(usage.fiveHour?.utilizationPct, 28)
        XCTAssertEqual(usage.fiveHour?.remainingPct, 72)
        XCTAssertEqual(usage.fiveHour?.resetsAt, DateParsing.iso8601("2026-09-07T10:14:00.000Z"))
        XCTAssertEqual(usage.sevenDay?.resetsAt, DateParsing.iso8601("2026-09-10T00:00:00Z"))
        XCTAssertEqual(usage.weekly(for: "claude-opus")?.utilizationPct, 61.2)
        XCTAssertEqual(usage.weekly(for: "claude-sonnet")?.utilizationPct, 39.5)
        XCTAssertNil(usage.sevenDayOpus?.resetsAt)
    }

    func testRejectsUnknownShape() {
        XCTAssertThrowsError(try ClaudeUsage.parse(Data(#"{"foo":1}"#.utf8)))
    }
}

final class ClaudeTranscriptTests: XCTestCase {
    static let user = #"{"parentUuid":null,"isSidechain":false,"cwd":"/Users/me/proj","sessionId":"s-1","type":"user","message":{"role":"user","content":"fix auth bug in middleware\nmore details"},"uuid":"u1","timestamp":"2026-09-07T05:41:44.123Z"}"#
    static let assistant = #"{"cwd":"/Users/me/proj","sessionId":"s-1","type":"assistant","message":{"id":"msg_1","role":"assistant","model":"claude-opus-4-5-20251101","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":12,"cache_creation_input_tokens":300,"cache_read_input_tokens":5000,"output_tokens":80}},"timestamp":"2026-09-07T05:42:10.000Z","entrypoint":"claude-desktop"}"#
    static let haiku = #"{"sessionId":"s-1","type":"assistant","message":{"role":"assistant","model":"claude-haiku-4-5","usage":{"input_tokens":5,"output_tokens":7}},"timestamp":"2026-09-07T05:43:00Z"}"#
    static let summary = #"{"type":"summary","summary":"Auth fix","leafUuid":"x"}"#

    func testParsesUserAndAssistantLines() throws {
        let events = ClaudeTranscriptParser.parse([Self.user, Self.assistant, Self.summary, "garbage"].joined(separator: "\n"))
        XCTAssertEqual(events.count, 2)
        XCTAssertEqual(events[0].role, .user)
        XCTAssertEqual(events[0].text, "fix auth bug in middleware\nmore details")
        XCTAssertEqual(events[0].cwd, "/Users/me/proj")
        XCTAssertEqual(events[1].role, .assistant)
        XCTAssertEqual(events[1].model, "claude-opus-4-5-20251101")
        XCTAssertEqual(events[1].tokensIn, 312)
        XCTAssertEqual(events[1].cacheReadTokens, 5000)
        XCTAssertEqual(events[1].outputTokens, 80)
        XCTAssertTrue(events[1].hasUsage)
        XCTAssertFalse(events[0].hasUsage)
        XCTAssertEqual(events[1].entrypoint, "claude-desktop")
        XCTAssertNil(events[0].entrypoint, "lines from builds without the field stay unlabelled")
    }

    func testTitleRules() {
        XCTAssertEqual(ClaudeTranscriptParser.title(from: "  fix auth bug\nmore"), "fix auth bug")
        XCTAssertNil(ClaudeTranscriptParser.title(from: "<command-name>/clear</command-name>"))
        XCTAssertNil(ClaudeTranscriptParser.title(from: "   "))
        XCTAssertEqual(ClaudeTranscriptParser.title(from: String(repeating: "a", count: 80))?.count, 60)
    }

    func testFastParserMatchesJSONParserOnRealShapes() throws {
        let nestedUsage = #"{"cwd":"/Users/me/proj","sessionId":"s-9","type":"assistant","requestId":"req_9","message":{"id":"msg_9","role":"assistant","model":"claude-fable-5-1","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"a}b{"}}],"usage":{"input_tokens":3,"cache_creation_input_tokens":11,"cache_read_input_tokens":2000,"output_tokens":40,"cache_creation":{"ephemeral_5m_input_tokens":11},"service_tier":"standard"}},"timestamp":"2026-09-07T05:50:00.000Z"}"#
        let toolResult = #"{"sessionId":"s-9","type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"huge \"quoted\" output {\"usage\":{\"fake\":1}}"}]},"timestamp":"2026-09-07T05:50:01.000Z"}"#
        let escapedUser = #"{"sessionId":"s-9","cwd":"/Users/me/中文 dir","type":"user","message":{"role":"user","content":"修复 \"auth\" bug\nline two"},"timestamp":"2026-09-07T05:49:00.000Z"}"#
        let lines = [Self.user, Self.assistant, Self.haiku, nestedUsage, toolResult, escapedUser, Self.summary, "garbage"].joined(separator: "\n") + "\n"

        let fast = FastTranscriptParser.parse(Data(lines.utf8))
        let slow = ClaudeTranscriptParser.parse(lines)
        XCTAssertEqual(fast.count, 6)
        XCTAssertEqual(fast.count, slow.count, "the summary line has no timestamp, so both parsers skip it")
        let fastWithUsage = fast.filter(\.hasUsage)
        let slowWithUsage = slow.filter(\.hasUsage)
        XCTAssertEqual(fastWithUsage.map(\.tokensIn), slowWithUsage.map(\.tokensIn))
        XCTAssertEqual(fastWithUsage.map(\.outputTokens), slowWithUsage.map(\.outputTokens))
        XCTAssertEqual(fastWithUsage.map(\.model), slowWithUsage.map(\.model))
        XCTAssertEqual(fastWithUsage.map(\.usageKey), slowWithUsage.map(\.usageKey))
        XCTAssertEqual(fast.map(\.stopReason), slow.map(\.stopReason))
        XCTAssertEqual(fast.map(\.isSidechain), slow.map(\.isSidechain))
        XCTAssertEqual(fast.map(\.isPrompt), slow.map(\.isPrompt))
        XCTAssertEqual(fast.map(\.entrypoint), slow.map(\.entrypoint))
        XCTAssertEqual(fast.map(\.isPrompt), [true, false, false, false, false, true], "typed prompts start turns; tool results do not")
        let nested = try XCTUnwrap(fast.first { $0.requestId == "req_9" })
        XCTAssertEqual(nested.tokensIn, 14)
        XCTAssertEqual(nested.messageId, "msg_9")
        let result = try XCTUnwrap(fast.first { $0.timestamp == DateParsing.iso8601("2026-09-07T05:50:01.000Z") })
        XCTAssertEqual(result.role, .user)
        XCTAssertNil(result.text, "tool results never become session titles")
        XCTAssertFalse(result.hasUsage, "usage text inside a tool result is not counted")
        let escaped = try XCTUnwrap(fast.first { $0.timestamp == DateParsing.iso8601("2026-09-07T05:49:00.000Z") })
        XCTAssertEqual(escaped.text, "修复 \"auth\" bug\nline two")
        XCTAssertEqual(escaped.cwd, "/Users/me/中文 dir")
        XCTAssertEqual(fast.first?.text, "fix auth bug in middleware\nmore details")
    }

    func testFastParserReadsRoleAndStopReasonWrittenAfterLongContent() throws {
        // Current Claude Code builds write `type`, `cwd` and `sessionId` after the message, which can exceed the prefix.
        let body = String(repeating: "x", count: 5000)
        let assistant = #"{"parentUuid":"p","isSidechain":false,"message":{"model":"claude-fable-5-1","id":"msg_long","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Write","input":{"content":"\#(body)","role":"user"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":9}},"requestId":"req_long","type":"assistant","uuid":"u1","timestamp":"2026-09-07T05:50:00.000Z","cwd":"/p","sessionId":"s-long"}"#
        let meta = #"{"parentUuid":"u1","isSidechain":false,"type":"user","message":{"role":"user","content":"<local-command-stdout>\#(body)</local-command-stdout>"},"isMeta":true,"uuid":"u2","timestamp":"2026-09-07T05:50:01.000Z","cwd":"/p","sessionId":"s-long"}"#
        let done = #"{"parentUuid":"u2","isSidechain":false,"message":{"model":"claude-fable-5-1","id":"msg_done","type":"message","role":"assistant","content":[{"type":"text","text":"\#(body)"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":4}},"requestId":"req_done","type":"assistant","uuid":"u3","timestamp":"2026-09-07T05:50:02.000Z","cwd":"/p","sessionId":"s-long","entrypoint":"cli"}"#
        let lines = [assistant, meta, done].joined(separator: "\n") + "\n"
        let fast = FastTranscriptParser.parse(Data(lines.utf8))
        let slow = ClaudeTranscriptParser.parse(lines)
        XCTAssertEqual(fast.map(\.role), [.assistant, .user, .assistant])
        XCTAssertEqual(fast.map(\.role), slow.map(\.role))
        XCTAssertEqual(fast.map(\.outputTokens), [9, 0, 4])
        XCTAssertEqual(fast.map(\.stopReason), ["tool_use", nil, "end_turn"])
        XCTAssertEqual(fast.map(\.isPrompt), [false, false, false], "command output is not a prompt even when flagged after its content")
        XCTAssertEqual(fast.map(\.messageId), ["msg_long", nil, "msg_done"])
        XCTAssertEqual(fast.map(\.entrypoint), [nil, nil, "cli"], "entrypoint follows the content, so it is read from the tail")
        XCTAssertEqual(fast.map(\.entrypoint), slow.map(\.entrypoint))
        XCTAssertEqual(fast.map(\.stopReason), slow.map(\.stopReason))
        XCTAssertEqual(fast.map(\.isPrompt), slow.map(\.isPrompt))
        XCTAssertEqual(fast.map(\.model), slow.map(\.model))
    }

    func testTurnStateEndsSessionsAtEndTurnOrInterruptionAndKeepsTheQuietLogFallback() throws {
        let base = DateParsing.iso8601("2026-09-07T06:00:00Z")!
        let iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        func user(_ text: String, at: TimeInterval) -> String {
            #"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"\#(text)"},"uuid":"u","timestamp":"\#(iso.format(base.addingTimeInterval(at)))","sessionId":"s-turn"}"#
        }
        func assistant(_ stop: String, id: String, at: TimeInterval) -> String {
            #"{"parentUuid":"u","isSidechain":false,"message":{"model":"claude-fable-5-1","id":"\#(id)","type":"message","role":"assistant","content":[{"type":"text","text":"…"}],"stop_reason":\#(stop),"usage":{"input_tokens":1,"output_tokens":1}},"type":"assistant","uuid":"a","timestamp":"\#(iso.format(base.addingTimeInterval(at)))","sessionId":"s-turn"}"#
        }
        var accumulator = TranscriptAccumulator(path: "/x/s-turn.jsonl", isSubagent: false)
        func live(after lines: [String], at: TimeInterval) -> Bool {
            accumulator.ingest(FastTranscriptParser.parse(Data((lines.joined(separator: "\n") + "\n").utf8)))
            return accumulator.build()!.isLive(now: base.addingTimeInterval(at), threshold: 120)
        }
        XCTAssertTrue(live(after: [user("build it", at: 0), assistant("\"tool_use\"", id: "msg_1", at: 5)], at: 60), "a tool call keeps the turn running")
        XCTAssertFalse(live(after: [], at: 200), "a quiet log still ends a stalled turn")
        XCTAssertFalse(live(after: [assistant("\"end_turn\"", id: "msg_2", at: 300)], at: 301), "end_turn ends the session at once, not two minutes later")
        XCTAssertTrue(live(after: [user("and the tests", at: 400)], at: 401), "the next prompt restarts it")
        XCTAssertFalse(live(after: [assistant("\"tool_use\"", id: "msg_3", at: 405), user("[Request interrupted by user for tool use]", at: 410)], at: 411), "an interruption ends the turn without a completion")
        XCTAssertEqual(accumulator.build()?.completions.map(\.id), [RecordCoding.hash(["Claude", "s-turn", "msg_2"])])
        var legacy = TranscriptAccumulator(path: "/x/old.jsonl", isSubagent: false)
        legacy.ingest(FastTranscriptParser.parse(Data((assistant("null", id: "msg_9", at: 0) + "\n").utf8)))
        XCTAssertTrue(legacy.build()!.isLive(now: base.addingTimeInterval(60), threshold: 120), "a build without stop reasons keeps the freshness rule")
        XCTAssertFalse(legacy.build()!.isLive(now: base.addingTimeInterval(600), threshold: 120))
    }

    func testStorePersistsAccumulatorsAcrossInstances() async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-cache-\(UUID().uuidString)", isDirectory: true)
        let projects = base.appendingPathComponent("projects/-p", isDirectory: true)
        try FileManager.default.createDirectory(at: projects, withIntermediateDirectories: true)
        let file = projects.appendingPathComponent("s-1.jsonl")
        try (Self.user + "\n" + Self.assistant + "\n").write(to: file, atomically: true, encoding: .utf8)
        let cache = base.appendingPathComponent("cache.json")

        let first = ClaudeTranscriptStore(roots: [projects.deletingLastPathComponent()], cacheURL: cache)
        let sessions = await first.sessions(modifiedSince: .distantPast)
        XCTAssertEqual(sessions.first?.tokensOut, 80)
        XCTAssertTrue(FileManager.default.fileExists(atPath: cache.path))

        // A second instance answers from the cache without re-reading unchanged files.
        try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: file.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: file.path) }
        let second = ClaudeTranscriptStore(roots: [projects.deletingLastPathComponent()], cacheURL: cache)
        let cached = await second.sessions(modifiedSince: .distantPast)
        XCTAssertEqual(cached.first?.tokensOut, 80)
        XCTAssertEqual(cached.first?.task, "fix auth bug in middleware")
    }

    func testDuplicateMessageLinesCountOnce() {
        let first = #"{"sessionId":"s-2","type":"assistant","requestId":"req_1","message":{"id":"msg_dup","role":"assistant","model":"claude-opus-4-5","content":[{"type":"text","text":"a"}],"usage":{"input_tokens":10,"output_tokens":20}},"timestamp":"2026-09-07T05:43:00Z"}"#
        let second = #"{"sessionId":"s-2","type":"assistant","requestId":"req_1","message":{"id":"msg_dup","role":"assistant","model":"claude-opus-4-5","content":[{"type":"tool_use","name":"Read"}],"usage":{"input_tokens":10,"output_tokens":20}},"timestamp":"2026-09-07T05:43:01Z"}"#
        var accumulator = TranscriptAccumulator(path: "/x/s-2.jsonl", isSubagent: false)
        accumulator.ingest(ClaudeTranscriptParser.parse([first, second].joined(separator: "\n")))
        let session = try! XCTUnwrap(accumulator.build())
        XCTAssertEqual(session.usage.count, 1)
        XCTAssertEqual(session.tokensOut, 20)
        XCTAssertEqual(ClaudeTranscriptParser.parseLine(first)?.usageKey, "m:msg_dup")
    }

    func testModelMapper() {
        XCTAssertEqual(ClaudeModelMapper.agentId(for: "claude-opus-4-5-20251101"), "claude-model:claude-opus-4-5-20251101")
        XCTAssertEqual(ClaudeModelMapper.agentId(for: "claude-sonnet-4-5"), "claude-model:claude-sonnet-4-5")
        XCTAssertEqual(ClaudeModelMapper.agentId(for: "claude-haiku-4-5"), "claude-model:claude-haiku-4-5")
        XCTAssertEqual(ClaudeModelMapper.agentId(for: "claude-fable-5-1"), "claude-model:claude-fable-5-1")
        XCTAssertEqual(ClaudeModelMapper.agentId(for: "custom-model"), "claude-model:custom-model")
        XCTAssertNil(ClaudeModelMapper.agentId(for: "<synthetic>"))
        XCTAssertNil(ClaudeModelMapper.agentId(for: nil))
    }

    func testModelInfoParsesVersions() {
        func name(_ id: String) -> String? { ClaudeModelInfo.parse(id)?.displayName }
        XCTAssertEqual(name("claude-opus-4-5-20251101"), "Opus 4.5")
        XCTAssertEqual(name("claude-opus-5"), "Opus 5")
        XCTAssertEqual(name("claude-fable-5-1"), "Fable 5.1")
        XCTAssertEqual(name("claude-sonnet-4-6"), "Sonnet 4.6")
        XCTAssertEqual(name("claude-haiku-4-5-20251001"), "Haiku 4.5")
        XCTAssertEqual(name("claude-3-5-haiku-20241022"), "Haiku 3.5")
        XCTAssertEqual(name("claude-3-7-sonnet-latest"), "Sonnet 3.7")
        XCTAssertEqual(name("us.anthropic.claude-3-5-sonnet-20241022-v2:0"), "Sonnet 3.5")
        XCTAssertNil(ClaudeModelInfo.parse("gpt-4o"))
        XCTAssertNil(ClaudeModelInfo.parse("<synthetic>"))
        XCTAssertTrue(ClaudeModelInfo.parse("claude-haiku-4-5")!.isSubagentModel)
    }

    func testDiscoveryIncludesEveryVersionAndSubagentModel() {
        let t0 = Date(timeIntervalSince1970: 1_000)
        let found = ClaudeModelDiscovery.discover([
            (modelId: "claude-sonnet-4-6", seenAt: t0),
            (modelId: "claude-fable-5-1", seenAt: t0.addingTimeInterval(50)),
            (modelId: "claude-sonnet-5", seenAt: t0.addingTimeInterval(100)),
            (modelId: "<synthetic>", seenAt: t0.addingTimeInterval(200)),
            (modelId: "claude-haiku-4-5", seenAt: t0.addingTimeInterval(10)),
        ])
        XCTAssertEqual(found.map(\.modelId), ["claude-sonnet-5", "claude-fable-5-1", "claude-haiku-4-5", "claude-sonnet-4-6"])
        XCTAssertEqual(found[0].descriptor.model, "Sonnet 5")
        XCTAssertTrue(found[0].descriptor.enabled)
        XCTAssertTrue(found.allSatisfy { $0.descriptor.enabled }, "all model consumers are included")
    }

    func testAccumulatorBuildsSessionWithDominantModel() {
        var accumulator = TranscriptAccumulator(path: "/x/s-1.jsonl", isSubagent: false)
        accumulator.ingest(ClaudeTranscriptParser.parse([Self.user, Self.assistant].joined(separator: "\n")))
        accumulator.ingest(ClaudeTranscriptParser.parse(Self.haiku))
        let session = try! XCTUnwrap(accumulator.build())
        XCTAssertEqual(session.id, "s-1")
        XCTAssertEqual(session.task, "fix auth bug in middleware")
        XCTAssertEqual(session.dominantAgentId, "claude-model:claude-opus-4-5-20251101")
        XCTAssertEqual(session.usage.count, 2)
        XCTAssertEqual(session.usage.map(\.agentId), ["claude-model:claude-opus-4-5-20251101", "claude-model:claude-haiku-4-5"], "each actual model keeps its own tokens")
        XCTAssertEqual(session.modelsSeen.keys.sorted(), ["claude-haiku-4-5", "claude-opus-4-5-20251101"])
        XCTAssertEqual(session.entrypoint, "claude-desktop", "kept from the last line that carried it")
        XCTAssertEqual(session.tokensIn, 312 + 5)
        XCTAssertEqual(session.tokensOut, 87)
        XCTAssertEqual(session.startedAt, DateParsing.iso8601("2026-09-07T05:41:44.123Z"))
        XCTAssertEqual(session.lastActivityAt, DateParsing.iso8601("2026-09-07T05:43:00Z"))
        XCTAssertTrue(session.isLive(now: session.lastActivityAt.addingTimeInterval(60), threshold: 120))
        XCTAssertFalse(session.isLive(now: session.lastActivityAt.addingTimeInterval(600), threshold: 120))
    }

    func testStoreReadsIncrementally() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-\(UUID().uuidString)/projects/-Users-me-proj", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("s-1.jsonl")
        try (Self.user + "\n").write(to: file, atomically: true, encoding: .utf8)
        let store = ClaudeTranscriptStore(root: dir.deletingLastPathComponent())
        let first = await store.sessions(modifiedSince: .distantPast)
        XCTAssertEqual(first.count, 1)
        XCTAssertEqual(first[0].usage.count, 0)

        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((Self.assistant + "\n").utf8))
        try handle.close()
        // Force a distinct modification date.
        try FileManager.default.setAttributes([.modificationDate: Date().addingTimeInterval(5)], ofItemAtPath: file.path)

        let second = await store.sessions(modifiedSince: .distantPast)
        XCTAssertEqual(second.count, 1)
        XCTAssertEqual(second[0].usage.count, 1)
        XCTAssertEqual(second[0].tokensOut, 80)
        let none = await store.sessions(modifiedSince: Date().addingTimeInterval(3600))
        XCTAssertEqual(none.count, 0)
    }
}

final class UsageAnalyticsTests: XCTestCase {
    var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Asia/Shanghai")!
        return calendar
    }

    let now = DateParsing.iso8601("2026-09-06T06:32:00Z")! // Sunday 14:32 CST

    func testHourlyHistoryCarriesForwardAndShowsResets() {
        // Buckets (5 hours ending in the current hour): 10:00, 11:00 (= base), 12:00, 13:00, 14:00.
        let base = UsageAnalytics.hourStart(now, calendar: calendar).addingTimeInterval(-3 * 3600)
        let quota = [
            QuotaSample(agentId: "a", timestamp: base.addingTimeInterval(600), remainingPct: 80),
            QuotaSample(agentId: "a", timestamp: base.addingTimeInterval(3000), remainingPct: 70),
            QuotaSample(agentId: "a", timestamp: base.addingTimeInterval(2 * 3600 + 60), remainingPct: 100),
        ]
        let usage = [
            UsageEvent(timestamp: base.addingTimeInterval(100), agentId: "a", tokensIn: 1000, tokensOut: 200),
            UsageEvent(timestamp: base.addingTimeInterval(100), agentId: "b", tokensIn: 999, tokensOut: 0),
        ]
        let history = UsageAnalytics.hourlyHistory(agentId: "a", quota: quota, usage: usage, hours: 5, now: now, calendar: calendar, fallbackRemaining: nil)
        XCTAssertEqual(history.count, 5)
        XCTAssertEqual(history[0].remainingStart, 80, "hours before the first sample take its value")
        XCTAssertEqual(history[0].tokens, 0)
        XCTAssertEqual(history[1].hourStart, base)
        XCTAssertEqual(history[1].remainingStart, 80)
        XCTAssertEqual(history[1].remainingEnd, 70)
        XCTAssertEqual(history[1].tokens, 1200)
        XCTAssertEqual(history[2].remainingStart, 70)
        XCTAssertEqual(history[2].remainingEnd, 70)
        XCTAssertEqual(history[3].remainingStart, 100, "reset shows as a jump")
        XCTAssertEqual(history[3].remainingEnd, 100)
        XCTAssertEqual(history[4].remainingEnd, 100)
        let path = ChartData.remainingPath(history)
        XCTAssertTrue(path.contains(CGPoint(x: 0.6, y: 70)) && path.contains(CGPoint(x: 0.6, y: 100)))
    }

    func testHourlyHistoryWithoutSamplesUsesFallback() {
        let history = UsageAnalytics.hourlyHistory(agentId: "a", quota: [], usage: [], hours: 2, now: now, calendar: calendar, fallbackRemaining: 42)
        XCTAssertEqual(history.map(\.remainingEnd), [42, 42])
    }

    func testActivityGridNormalises() {
        let monday = DateParsing.iso8601("2026-08-31T02:00:00Z")! // Monday 10:00 CST
        let usage = [
            UsageEvent(timestamp: monday, agentId: "a", tokensIn: 400, tokensOut: 100),
            UsageEvent(timestamp: monday.addingTimeInterval(3600), agentId: "a", tokensIn: 200, tokensOut: 50),
        ]
        let grid = UsageAnalytics.activityGrid(usage: usage, since: .distantPast, calendar: calendar)
        XCTAssertEqual(grid.rows[0][10], 1)
        XCTAssertEqual(grid.rows[0][11], 0.5)
        XCTAssertEqual(grid.rows[6][10], 0)
        XCTAssertEqual(grid.tokens[0][10], 500)
        XCTAssertEqual(grid.tokens[0][11], 250)
        XCTAssertEqual(grid.tokens[6][10], 0)
    }

    func testActivityGridRetainsEveryModelAndVersion() {
        let monday = DateParsing.iso8601("2026-08-31T02:00:00Z")!
        let usage: [UsageEvent] = [
            .init(timestamp: monday, agentId: "claude-model:opus-4-8", tokensIn: 80, tokensOut: 20),
            .init(timestamp: monday.addingTimeInterval(60), agentId: "claude-model:opus-4-8", tokensIn: 8, tokensOut: 2),
            .init(timestamp: monday, agentId: "claude-model:opus-5", tokensIn: 150, tokensOut: 50),
            .init(timestamp: monday, agentId: "codex-model:new-model", tokensIn: 1, tokensOut: 0),
            .init(timestamp: monday.addingTimeInterval(-1), agentId: "outside-range", tokensIn: 999, tokensOut: 0),
        ]
        let grid = UsageAnalytics.activityGrid(usage: usage, since: monday, calendar: calendar)
        XCTAssertEqual(grid.tokensByModel[0][10], ["claude-model:opus-4-8": 110, "claude-model:opus-5": 200, "codex-model:new-model": 1])
        XCTAssertEqual(grid.tokens[0][10], 311)
        XCTAssertEqual(grid.rows[0][10], 1)
        XCTAssertEqual(grid.tokensByModel[0][11], [:])
        XCTAssertEqual(grid.tokens.flatMap { $0 }.reduce(0, +), 311)
    }

    func testBurnRateStartsAfterAnObservedReset() {
        let samples = [
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-3600), remainingPct: 50),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-2700), remainingPct: 45),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-1800), remainingPct: 100),
            QuotaSample(agentId: "a", timestamp: now, remainingPct: 98),
        ]
        let cycle = QuotaCycle(resetAt: now.addingTimeInterval(3600), duration: 5 * 3600)
        let rate = UsageAnalytics.burnRate(samples: samples, cycle: cycle, now: now)
        XCTAssertEqual(rate?.pctPerHour ?? 0, 4, accuracy: 1e-9)
        XCTAssertNil(UsageAnalytics.burnRate(samples: Array(samples.prefix(1)), cycle: cycle, now: now))
    }

    func testCapStats() {
        let samples = [
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-7200), remainingPct: 3),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-7000), remainingPct: 0),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-6400), remainingPct: 0),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-5800), remainingPct: 100),
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-300), remainingPct: 0.2),
        ]
        let stats = UsageAnalytics.capStats(samples: samples, now: now)
        XCTAssertEqual(stats.hits, 2)
        XCTAssertEqual(stats.totalWait, 1200 + 300, accuracy: 1e-6)
        XCTAssertEqual(stats.longestWait, 1200, accuracy: 1e-6)
        XCTAssertEqual(stats.longestAt, now.addingTimeInterval(-7000))
        XCTAssertEqual(UsageAnalytics.capStats(samples: [], now: now), .none)
    }

    func testWeeklyShare() {
        let usage = [
            UsageEvent(timestamp: now, agentId: "a", tokensIn: 30, tokensOut: 0),
            UsageEvent(timestamp: now, agentId: "b", tokensIn: 10, tokensOut: 0),
        ]
        let share = UsageAnalytics.weeklyShare(usage: usage)
        XCTAssertEqual(share["a"] ?? 0, 0.75, accuracy: 1e-9)
        XCTAssertEqual(share["b"] ?? 0, 0.25, accuracy: 1e-9)
        XCTAssertEqual(UsageAnalytics.weeklyShare(usage: []), [:])
    }
}

final class QuotaHistoryStoreTests: XCTestCase {
    func testPersistsAcrossInstances() async {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-\(UUID().uuidString)/quota.json")
        let now = Date()
        let store = QuotaHistoryStore(fileURL: url)
        await store.append([
            QuotaSample(agentId: "a", timestamp: now.addingTimeInterval(-40 * 86400), remainingPct: 1),
            QuotaSample(agentId: "a", timestamp: now, remainingPct: 72),
        ], now: now)
        let reloaded = QuotaHistoryStore(fileURL: url)
        let samples = await reloaded.samples(agentId: "a", since: .distantPast)
        XCTAssertEqual(samples.count, 1, "samples older than the retention window are pruned")
        XCTAssertEqual(samples.first?.remainingPct, 72)
    }
}

final class ClaudeCodeProviderTests: XCTestCase {
    override func setUp() {
        super.setUp()
        L10n.setLanguage(.zhHans)
    }

    override func tearDown() {
        L10n.setLanguage(.system)
        super.tearDown()
    }

    private func fakeEngine(script: String) throws -> ClaudeEngineUsageClient {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-engine-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let fake = dir.appendingPathComponent("claude")
        try script.write(to: fake, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fake.path)
        return ClaudeEngineUsageClient(executable: fake, workingDirectory: dir, timeout: 10)
    }

    /// An engine that answers `get_usage` with the given `rate_limits` object.
    private func fakeEngine(rateLimits: String, subscription: String = "max") throws -> ClaudeEngineUsageClient {
        let response = #"{"type":"control_response","response":{"subtype":"success","request_id":"agent-hud-usage","response":{"subscription_type":"\#(subscription)","rate_limits_available":true,"rate_limits":\#(rateLimits)}}}"#
        return try fakeEngine(script: "#!/bin/bash\nread -r l\necho '\(response)'\n")
    }

    func testBuildsReportFromMocks() async throws {
        let now = Date()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-\(UUID().uuidString)/projects", isDirectory: true)
        let project = root.appendingPathComponent("-Users-me-proj", isDirectory: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        func line(_ ts: Date, model: String, out: Int, session: String, text: String? = nil) -> String {
            if let text {
                return #"{"sessionId":"\#(session)","cwd":"/Users/me/proj","type":"user","message":{"role":"user","content":"\#(text)"},"timestamp":"\#(iso.format(ts))"}"#
            }
            return #"{"sessionId":"\#(session)","cwd":"/Users/me/proj","type":"assistant","message":{"role":"assistant","model":"\#(model)","usage":{"input_tokens":100,"output_tokens":\#(out)}},"timestamp":"\#(iso.format(ts))"}"#
        }
        let done = #"{"sessionId":"live","cwd":"/Users/me/proj","type":"assistant","message":{"id":"msg_done","role":"assistant","model":"claude-opus-4-5","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":0,"output_tokens":0}},"timestamp":"\#(iso.format(now.addingTimeInterval(-1390)))"}"#
        let live = [
            line(now.addingTimeInterval(-1500), model: "", out: 0, session: "live", text: "fix auth bug"),
            line(now.addingTimeInterval(-1400), model: "claude-opus-4-5", out: 900, session: "live"),
            done,
            line(now.addingTimeInterval(-1350), model: "", out: 0, session: "live", text: "now the tests"),
            line(now.addingTimeInterval(-30), model: "claude-opus-4-5", out: 100, session: "live"),
        ].joined(separator: "\n") + "\n"
        let ended = [
            line(now.addingTimeInterval(-7200), model: "", out: 0, session: "old", text: "optimize db"),
            line(now.addingTimeInterval(-7000), model: "claude-sonnet-4-5", out: 300, session: "old"),
        ].joined(separator: "\n") + "\n"
        try live.write(to: project.appendingPathComponent("live.jsonl"), atomically: true, encoding: .utf8)
        try ended.write(to: project.appendingPathComponent("old.jsonl"), atomically: true, encoding: .utf8)

        let resets = now.addingTimeInterval(2 * 3600 + 14 * 60)
        let weeklyResets = now.addingTimeInterval(2 * 86400)
        let body = #"{"five_hour":{"utilization":28,"resets_at":"\#(iso.format(resets))"},"seven_day":{"utilization":39,"resets_at":"\#(iso.format(weeklyResets))"},"seven_day_opus":{"utilization":61,"resets_at":"\#(iso.format(weeklyResets))"}}"#
        let history = QuotaHistoryStore(fileURL: nil)
        await history.append([
            QuotaSample(agentId: ClaudeUsage.sessionRowId, timestamp: now.addingTimeInterval(-2 * 3600), remainingPct: 80),
            QuotaSample(agentId: ClaudeUsage.weeklyRowId, timestamp: now.addingTimeInterval(-5 * 86400), remainingPct: 100),
            QuotaSample(agentId: "claude-weekly-opus", timestamp: now.addingTimeInterval(-2 * 86400), remainingPct: 90)
        ], now: now)
        let provider = ClaudeCodeProvider(
            engine: try fakeEngine(rateLimits: body),
            transcripts: ClaudeTranscriptStore(root: root),
            history: history,
            clock: { now }
        )
        let report = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        XCTAssertNil(report.notice)
        XCTAssertEqual(report.subscriptionType, "max")

        XCTAssertEqual(report.snapshots.map(\.agentId), ["claude-session", "claude-weekly", "claude-weekly-opus"], "one row per quota window")
        XCTAssertEqual(report.discoveredAgents.map(\.model), ["window.session", "window.weekly", "window.weekly.Opus"])
        XCTAssertEqual(report.discoveredAgents.map { L10n.modelLabel($0.model) }, ["当前会话 · 5h", "本周 · 全部模型", "本周 · Opus"])
        XCTAssertEqual(report.snapshot(for: "claude-session")?.remainingPct, 72)
        XCTAssertEqual(report.snapshot(for: "claude-session")?.windowDuration, 5 * 3600)
        XCTAssertEqual(report.snapshot(for: "claude-weekly")?.windowDuration, 7 * 86400)
        XCTAssertEqual(report.snapshot(for: "claude-weekly-opus")?.windowDuration, 7 * 86400)
        XCTAssertEqual(report.snapshot(for: "claude-weekly")?.remainingPct, 61)
        XCTAssertEqual(report.snapshot(for: "claude-weekly-opus")?.remainingPct, 39)
        XCTAssertEqual(report.snapshot(for: "claude-session")?.resetAt?.timeIntervalSince1970 ?? 0, resets.timeIntervalSince1970, accuracy: 0.01)
        XCTAssertEqual(report.consumers.map(\.id), ["claude-model:claude-opus-4-5", "claude-model:claude-sonnet-4-5"], "models seen in transcripts, most recent first")

        XCTAssertEqual(report.sessions.count, 2)
        XCTAssertEqual(report.completions.count, 1)
        let completion = try XCTUnwrap(report.completions.first)
        XCTAssertEqual(completion.vendor, "Claude")
        XCTAssertEqual(completion.sessionID, "live")
        XCTAssertEqual(completion.task, "fix auth bug")
        XCTAssertEqual(completion.model, "Opus 4.5")
        XCTAssertEqual(try XCTUnwrap(completion.startedAt).timeIntervalSince1970, now.addingTimeInterval(-1500).timeIntervalSince1970, accuracy: 0.001)
        XCTAssertEqual(completion.completedAt.timeIntervalSince1970, now.addingTimeInterval(-1390).timeIntervalSince1970, accuracy: 0.001)
        XCTAssertEqual(report.sessions[0].id, "live")
        XCTAssertTrue(report.sessions[0].isLive)
        XCTAssertEqual(report.sessions[0].task, "fix auth bug")
        XCTAssertEqual(report.sessions[0].agentId, "claude-model:claude-opus-4-5")
        XCTAssertEqual(report.sessions[0].terminal, "proj")
        XCTAssertEqual(report.sessions[0].tokensOut, 1000)
        XCTAssertEqual(report.consumerIdsByQuota["claude-session"], ["claude-model:claude-opus-4-5", "claude-model:claude-sonnet-4-5"])
        XCTAssertEqual(report.consumerIdsByQuota["claude-weekly-opus"], ["claude-model:claude-opus-4-5"])
        XCTAssertFalse(report.sessions[1].isLive)
        XCTAssertEqual(report.sessions[1].agentId, "claude-model:claude-sonnet-4-5")
        // Both sessions fall inside the 5h window: live has 1200 of 1600 tokens → 75 % of the 28 % used.
        XCTAssertEqual(try XCTUnwrap(report.sessions[0].pctOfWindow), 21, accuracy: 1e-6)

        XCTAssertEqual(report.history.count, 48 * 3)
        XCTAssertEqual(report.history(for: "claude-session").last?.remainingEnd, 72)
        XCTAssertEqual(report.consumption.filter { $0.agentId == "claude-model:claude-opus-4-5" }.map(\.total).reduce(0, +), 1200)
        let eventTimes = report.consumption.filter { $0.agentId == "claude-model:claude-opus-4-5" }.map(\.timestamp)
        let expectedTimes = [-1400.0, -30.0].map { DateParsing.iso8601(iso.format(now.addingTimeInterval($0)))! }
        XCTAssertEqual(eventTimes.count, expectedTimes.count)
        for (actual, expected) in zip(eventTimes, expectedTimes) {
            XCTAssertEqual(actual.timeIntervalSince1970, expected.timeIntervalSince1970, accuracy: 0.001)
        }
        XCTAssertEqual(report.insights.windowUsedPct, 28)
        XCTAssertEqual(report.insights.windowSessionCount, 2)
        XCTAssertEqual(report.insights.weeklyShare["claude-model:claude-opus-4-5"] ?? 0, 0.75, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(report.insights.burnRatePctPerHour), 4, accuracy: 1e-6)
        XCTAssertEqual(try XCTUnwrap(report.insightsByAgent["claude-weekly"]?.burnRatePctPerHour), 39.0 / 120, accuracy: 1e-6,
                       "the weekly forecast includes readings older than the selected 48-hour statistics range")
        XCTAssertEqual(try XCTUnwrap(report.insightsByAgent["claude-weekly-opus"]?.burnRatePctPerHour), 51.0 / 48, accuracy: 1e-6)
        XCTAssertEqual(report.activity.rows.count, 7)
        _ = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        let stored = await history.samples(agentId: ClaudeUsage.sessionRowId, since: .distantPast)
        XCTAssertEqual(stored.count, 2, "each engine observation appends one sample; a poll served from the cache does not")
    }

    func testEngineModeUsesGetUsageAndDiscoversFromTranscripts() async throws {
        // Keep the clock inside the recorded response's window, regardless of the test's wall-clock date.
        let now = DateParsing.iso8601("2026-09-07T10:00:00Z")!
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-\(UUID().uuidString)/projects/-p", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        let line = #"{"sessionId":"f","cwd":"/p","type":"assistant","message":{"id":"m1","role":"assistant","model":"claude-fable-5-1","usage":{"input_tokens":10,"output_tokens":5}},"timestamp":"\#(iso.format(now.addingTimeInterval(-60)))"}"#
        try (line + "\n").write(to: root.appendingPathComponent("f.jsonl"), atomically: true, encoding: .utf8)
        let engine = try fakeEngine(script: "#!/bin/bash\nread -r l\necho '\(ClaudeEngineTests.response)'\n")
        let provider = ClaudeCodeProvider(
            engine: engine,
            transcripts: ClaudeTranscriptStore(root: root.deletingLastPathComponent()),
            history: QuotaHistoryStore(fileURL: nil),
            clock: { now }
        )
        let report = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        XCTAssertNil(report.notice)
        XCTAssertEqual(report.subscriptionType, "max")
        XCTAssertEqual(report.discoveredAgents.map(\.id), ["claude-session", "claude-weekly"])
        XCTAssertEqual(report.consumers.map(\.id), ["claude-model:claude-fable-5-1"])
        XCTAssertEqual(report.consumers.first?.model, "Fable 5.1")
        XCTAssertEqual(report.snapshot(for: "claude-session")?.remainingPct, 79)
        XCTAssertEqual(report.snapshot(for: "claude-weekly")?.remainingPct, 30)
        XCTAssertEqual(report.sessions.first?.agentId, "claude-model:claude-fable-5-1")
    }

    func testReturnsAllIndexedSessionsForRangeFiltering() async throws {
        let now = Date()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-sessions-\(UUID().uuidString)")
        let project = root.appendingPathComponent("project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for index in 0..<12 {
            let line: [String: Any] = [
                "sessionId": "session-\(index)", "type": "user",
                "message": ["role": "user", "content": "Task \(index)"],
                "timestamp": now.addingTimeInterval(-Double(index) * 3600).ISO8601Format(),
            ]
            var data = try JSONSerialization.data(withJSONObject: line)
            data.append(0x0a)
            try data.write(to: project.appendingPathComponent("\(index).jsonl"))
        }
        let provider = ClaudeCodeProvider(
            engine: try fakeEngine(rateLimits: #"{"five_hour":{"utilization":10}}"#),
            transcripts: ClaudeTranscriptStore(root: root), history: QuotaHistoryStore(fileURL: nil), clock: { now }
        )
        let report = try await provider.fetchAccountAndLocalUsage(agents: [], historyHours: 169)
        XCTAssertEqual(report.sessions.count, 12, "the view controls how many sessions are expanded")
        XCTAssertEqual(report.sessions.first?.id, "session-0")
        XCTAssertEqual(report.sessions.last?.id, "session-11")
    }

    func testUnavailablePlanLimitsKeepLocalDataWithNotice() async throws {
        let now = Date()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-\(UUID().uuidString)/projects/-p", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let line = #"{"sessionId":"k","cwd":"/p","type":"user","message":{"role":"user","content":"api key session"},"timestamp":"\#(now.addingTimeInterval(-60).ISO8601Format())"}"#
        try (line + "\n").write(to: root.appendingPathComponent("k.jsonl"), atomically: true, encoding: .utf8)
        let response = #"{"type":"control_response","response":{"subtype":"success","request_id":"x","response":{"subscription_type":null,"rate_limits_available":false,"rate_limits":null}}}"#
        let provider = ClaudeCodeProvider(
            engine: try fakeEngine(script: "#!/bin/bash\nread -r l\necho '\(response)'\n"),
            transcripts: ClaudeTranscriptStore(root: root.deletingLastPathComponent()),
            history: QuotaHistoryStore(fileURL: nil),
            clock: { now }
        )
        let report = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
        XCTAssertEqual(report.notice, "当前登录方式没有订阅额度（API key 或第三方平台）")
        XCTAssertTrue(report.snapshots.isEmpty)
        XCTAssertEqual(report.sessions.map(\.id), ["k"], "sessions still come from local logs")
    }

    func testEngineFailureThrows() async throws {
        let provider = ClaudeCodeProvider(
            engine: try fakeEngine(script: "#!/bin/bash\nread -r l\necho boom >&2\nexit 1\n"),
            transcripts: ClaudeTranscriptStore(root: FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-empty-\(UUID().uuidString)")),
            history: QuotaHistoryStore(fileURL: nil)
        )
        do {
            _ = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
            XCTFail("expected failure")
        } catch let error as ClaudeDataError {
            XCTAssertEqual(error, .engineFailed("boom\n"))
        } catch {
            XCTFail("unexpected \(error)")
        }
    }

    func testEngineMissingThrows() async {
        let provider = ClaudeCodeProvider(
            engine: nil,
            transcripts: ClaudeTranscriptStore(root: FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-empty-\(UUID().uuidString)")),
            history: QuotaHistoryStore(fileURL: nil)
        )
        do {
            _ = try await provider.fetchAccountAndLocalUsage(agents: DefaultAgents.list, historyHours: 48)
            XCTFail("expected failure")
        } catch let error as ClaudeDataError {
            XCTAssertEqual(error, .engineNotFound)
        } catch {
            XCTFail("unexpected \(error)")
        }
    }
}

final class CooperativeIndexingTests: XCTestCase {
    func testIndexStepHonoursBudgetNewestFirstThenCompletes() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-index-\(UUID().uuidString)/projects/-p", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        let now = Date()
        for i in 0..<6 {
            let ts = iso.format(now.addingTimeInterval(TimeInterval(-i * 600)))
            let line = #"{"sessionId":"s\#(i)","cwd":"/p","type":"assistant","message":{"id":"m\#(i)","role":"assistant","model":"claude-sonnet-5","usage":{"input_tokens":1,"output_tokens":\#(i + 1)}},"timestamp":"\#(ts)"}"#
            let file = root.appendingPathComponent("s\(i).jsonl")
            try (line + "\n").write(to: file, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(TimeInterval(-i * 600))], ofItemAtPath: file.path)
        }
        let store = ClaudeTranscriptStore(root: root.deletingLastPathComponent())
        let first = await store.index(modifiedSince: .distantPast, timeBudget: 0)
        XCTAssertEqual(first.sessions.count, 1, "a zero budget still reads the newest file")
        XCTAssertEqual(first.sessions.first?.id, "s0")
        XCTAssertEqual(first.pending, 5)

        let second = await store.index(modifiedSince: .distantPast, timeBudget: 0)
        XCTAssertEqual(second.sessions.map(\.id).sorted(), ["s0", "s1"])
        XCTAssertEqual(second.pending, 4)

        let all = await store.index(modifiedSince: .distantPast, timeBudget: 10)
        XCTAssertEqual(all.sessions.count, 6)
        XCTAssertEqual(all.pending, 0)
        let again = await store.index(modifiedSince: .distantPast, timeBudget: 0)
        XCTAssertEqual(again.sessions.count, 6, "unchanged files need no work")
        XCTAssertEqual(again.pending, 0)
        let stats = await store.lastScan
        XCTAssertEqual(stats.filesRead, 0)
    }
}

final class AccumulatorCompactionTests: XCTestCase {
    func testCompactionDropsDedupeSetOnlyForIdleFiles() {
        let line = ClaudeTranscriptTests.assistant
        var fresh = TranscriptAccumulator(path: "/x/a.jsonl", isSubagent: false)
        fresh.ingest(ClaudeTranscriptParser.parse(line))
        let stamp = DateParsing.iso8601("2026-09-07T05:42:10.000Z")!
        fresh.compactIfFinished(now: stamp.addingTimeInterval(3600))
        // Still active: a duplicate of the same message is still recognised.
        fresh.ingest(ClaudeTranscriptParser.parse(line))
        XCTAssertEqual(fresh.build()?.usage.count, 1)

        var old = TranscriptAccumulator(path: "/x/b.jsonl", isSubagent: false)
        old.ingest(ClaudeTranscriptParser.parse(line))
        old.compactIfFinished(now: stamp.addingTimeInterval(3 * 86400))
        old.ingest(ClaudeTranscriptParser.parse(line))
        XCTAssertEqual(old.build()?.usage.count, 2, "after compaction the set is gone; finished files never get appended anyway")
    }
}
