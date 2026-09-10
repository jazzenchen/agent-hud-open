import XCTest
@testable import AgentHUDCore

final class DeepSeekProviderTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1788787140)
    private static let balanceJSON = #"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"8.85","granted_balance":"0.00","topped_up_balance":"8.85"}]}"#

    func testChunkAndFinalUsageReplaceOneAttemptAndDoNotRecountReasoningOrCache() throws {
        var transcript = DeepSeekTranscript()
        try feed(&transcript, header())
        try feed(&transcript, event("turn/start", seq: 0, data: ["turn": 1]))
        try feed(&transcript, event("step/start", seq: 1, data: ["turn": 1, "step": 1]))
        try feed(&transcript, model(seq: 2))
        try feed(&transcript, usage(seq: 3, input: 8086, output: 40, cached: 1000, chunk: true))
        try feed(&transcript, usage(seq: 4, input: 8086, output: 58, cached: 1000))
        XCTAssertEqual(transcript.usage.count, 1)
        XCTAssertEqual(transcript.usage[0].input, 8086)
        XCTAssertEqual(transcript.usage[0].output, 58)
        XCTAssertEqual(transcript.usage[0].cachedInput, 1000)
        XCTAssertEqual(transcript.usage[0].model, "deepseek-v4-flash")
        XCTAssertTrue(transcript.isLive(now: now, modifiedAt: now))
        try feed(&transcript, event("turn/end", seq: 5, data: ["turn": 1, "reason": ["kind": "completed"]]))
        try feed(&transcript, event("session/title", seq: 6, data: ["title": "Updated title"]))
        XCTAssertFalse(transcript.isLive(now: now, modifiedAt: now))
        XCTAssertEqual(transcript.title, "Updated title")
    }

    func testRetryPreservesFailedAttemptAndCountsNextModelSeparately() throws {
        var transcript = DeepSeekTranscript()
        for line in [header(), model(seq: 0), usage(seq: 1, input: 100, output: 10, chunk: true),
                     event("llm/retry-started", seq: 2, data: ["turn": 1, "step": 1]),
                     model(seq: 3, name: "deepseek-v4-pro"), usage(seq: 4, input: 200, output: 20),
                     usage(seq: 5, input: 200, output: 20)] { try feed(&transcript, line) }
        XCTAssertEqual(transcript.usage.map(\.input), [100, 200])
        XCTAssertEqual(transcript.usage.map(\.model), ["deepseek-v4-flash", "deepseek-v4-pro"])
    }

    func testForkUsesSeedLengthRatherThanTimestampAndDoesNotInheritLiveness() throws {
        var transcript = DeepSeekTranscript()
        for line in [header(id: "child", seed: 3, child: true), model(seq: 0),
                     event("turn/start", seq: 1, data: ["turn": 1]), usage(seq: 2),
                     usage(seq: 3, input: 5, output: 2)] { try feed(&transcript, line) }
        XCTAssertTrue(transcript.isSubagent)
        XCTAssertFalse(transcript.isLive(now: now, modifiedAt: now))
        XCTAssertEqual(transcript.usage.map(\.input), [5])
        XCTAssertEqual(transcript.usage[0].model, "deepseek-v4-flash")
    }

    func testAllTurnEndReasonsAndStaleActivityStopBreathing() throws {
        for reason in ["completed", "aborted", "error", "blocked", "interrupted", "max-tokens"] {
            var transcript = DeepSeekTranscript()
            try feed(&transcript, header())
            try feed(&transcript, event("turn/start", seq: 0, data: ["turn": 1]))
            XCTAssertFalse(transcript.isLive(now: now.addingTimeInterval(121), modifiedAt: now))
            try feed(&transcript, event("turn/end", seq: 1, data: ["turn": 1, "reason": ["kind": reason]]))
            XCTAssertFalse(transcript.isLive(now: now, modifiedAt: now), reason)
        }
    }

    func testIncrementalCacheRetriesPartialLineAndDeduplicatesCopiedSessions() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let root = dir.appendingPathComponent("sessions")
        let file = try logFile(root, id: "first")
        let countLine = usage(seq: 1)
        try (header() + "\n" + model(seq: 0) + "\n" + String(countLine.prefix(30))).write(to: file, atomically: true, encoding: .utf8)
        let cache = dir.appendingPathComponent("cache.json")
        let store = DeepSeekTranscriptStore(root: root, cacheURL: cache)
        let first = await store.index(since: .distantPast)
        XCTAssertEqual(first.sessions.count, 1)
        XCTAssertTrue(first.sessions[0].transcript.usage.isEmpty)
        XCTAssertNil(first.indexing)
        try append(String(countLine.dropFirst(30)) + "\n", to: file)
        let second = await store.index(since: .distantPast)
        XCTAssertEqual(second.sessions[0].transcript.usage.count, 1)
        let copy = try logFile(root, id: "copy")
        try FileManager.default.copyItem(at: file, to: copy)
        let restored = DeepSeekTranscriptStore(root: root, cacheURL: cache)
        let third = await restored.index(since: .distantPast)
        XCTAssertEqual(third.sessions.count, 1)
        XCTAssertEqual(third.sessions[0].transcript.usage[0].input, 8086)
        try (header(id: "replacement") + "\n").write(to: file, atomically: true, encoding: .utf8)
        let fourth = await store.index(since: .distantPast)
        XCTAssertTrue(fourth.sessions.contains { $0.transcript.id == "replacement" && $0.transcript.usage.isEmpty })
    }

    func testConcatenatedZstandardFramesAndTornFinalFrameMatchPlaintext() async throws {
        guard DeepSeekLocator.nodeExecutable() != nil else { throw XCTSkip("Harness requires Node.js with Zstandard support") }
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let lines = [header(), event("turn/start", seq: 0, data: ["turn": 1]), model(seq: 1), usage(seq: 2),
                     event("turn/end", seq: 3, data: ["turn": 1, "reason": ["kind": "completed"]])]
        let plain = dir.appendingPathComponent("fixture.txt")
        try (lines.joined(separator: "\n") + "\n").write(to: plain, atomically: true, encoding: .utf8)
        let frames = try DeepSeekNode.run(script: """
        const fs = require('node:fs'), z = require('node:zlib');
        for (const line of fs.readFileSync(process.argv[1], 'utf8').trimEnd().split('\\n'))
          process.stdout.write(z.zstdCompressSync(Buffer.from(line + '\\n')));
        """, arguments: [plain.path])
        let file = dir.appendingPathComponent("session.jsonl.zstd")
        try frames.write(to: file)
        XCTAssertEqual(try DeepSeekLogReader.read(file), try Data(contentsOf: plain), "all frames, not just the session header")
        let store = DeepSeekTranscriptStore(root: dir)
        let full = await store.index(since: .distantPast, timeBudget: 5)
        XCTAssertNil(full.notice)
        XCTAssertEqual(full.sessions[0].transcript.usage[0].input, 8086)
        XCTAssertFalse(full.sessions[0].transcript.isLive(now: now, modifiedAt: now))
        try Data(frames.dropLast(8)).write(to: file)
        let partial = await store.index(since: .distantPast, timeBudget: 5)
        XCTAssertNil(partial.notice)
        XCTAssertEqual(partial.sessions[0].transcript.usage.count, 1)
        try frames.write(to: file)
        let complete = await store.index(since: .distantPast, timeBudget: 5)
        XCTAssertEqual(complete.sessions[0].transcript.usage.count, 1)
        XCTAssertFalse(complete.sessions[0].transcript.isLive(now: now, modifiedAt: now))
    }

    func testLargeCompressedLogDrainsPipeAndSkipsPackedConversationContent() throws {
        guard DeepSeekLocator.nodeExecutable() != nil else { throw XCTSkip("Harness requires Node.js") }
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("session.jsonl.zstd")
        let frames = try DeepSeekNode.run(script: """
        const z = require('node:zlib');
        process.stdout.write(z.zstdCompressSync(Buffer.from('x'.repeat(300000))));
        process.stdout.write(z.zstdCompressSync(Buffer.from('end')));
        """, arguments: [])
        try frames.write(to: file)
        let decoded = try DeepSeekLogReader.read(file)
        XCTAssertEqual(decoded.count, 300003)
        XCTAssertTrue(String(decoding: decoded.suffix(3), as: UTF8.self) == "end")
        var transcript = DeepSeekTranscript()
        try feed(&transcript, header())
        try feed(&transcript, json(["type": "text-chunks", "seq": 0, "text": String(repeating: "private conversation ", count: 100)]))
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(transcript), as: UTF8.self).contains("private conversation"))
    }

    func testUnknownSchemaReportsFailureWithoutClaimingACompleteSession() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("session.jsonl")
        try "{\"type\":\"session\",\"version\":999,\"id\":\"future\",\"createdAt\":0}\n".write(to: file, atomically: true, encoding: .utf8)
        let store = DeepSeekTranscriptStore(root: dir)
        let result = await store.index(since: .distantPast)
        XCTAssertNotNil(result.notice)
        XCTAssertTrue(result.sessions.isEmpty)
    }

    func testBalanceSchemaPreservesExactAmountsAndRejectsInvalidNumbers() throws {
        let balance = try JSONDecoder().decode(DeepSeekBalance.self, from: Data(Self.balanceJSON.utf8))
        XCTAssertTrue(balance.isAvailable)
        XCTAssertEqual(balance.balances[0].total, Decimal(string: "8.85"))
        XCTAssertEqual(balance.balances[0].granted, 0)
        for invalid in ["nan", "8.85junk", ""] {
            XCTAssertThrowsError(try JSONDecoder().decode(DeepSeekBalance.self, from: Data(Self.balanceJSON.replacingOccurrences(of: "8.85", with: invalid).utf8)))
        }
        let empty = try JSONDecoder().decode(DeepSeekBalance.self, from: Data(#"{"is_available":false,"balance_infos":[]}"#.utf8))
        XCTAssertFalse(empty.isAvailable)
        XCTAssertTrue(empty.balances.isEmpty)
    }

    func testPricingAccountsForCurrencyModelCacheAndPeakBoundaries() throws {
        let formatter = ISO8601DateFormatter()
        for (instant, peak) in [("2026-09-07T00:59:59Z", false), ("2026-09-07T01:00:00Z", true),
                                ("2026-09-07T04:00:00Z", false), ("2026-09-07T06:00:00Z", true),
                                ("2026-09-07T10:00:00Z", false), ("2026-09-12T02:00:00Z", false)] {
            XCTAssertEqual(DeepSeekPricing.isPeak(formatter.date(from: instant)!), peak, instant)
        }
        let sample = DeepSeekTranscript.Usage(timestamp: now, requestedAt: now, provider: "deepseek-official",
                                             model: "deepseek-v4-flash", input: 8086, cachedInput: 0, output: 58)
        XCTAssertEqual(DeepSeekPricing.estimate(sample, currency: "CNY"), Decimal(string: "0.01239"))
        XCTAssertEqual(DeepSeekPricing.estimate(sample, currency: "USD"), Decimal(string: "0.0018172"))
        XCTAssertNil(DeepSeekPricing.estimate(sample, currency: "EUR"))
        let peak = DeepSeekTranscript.Usage(timestamp: now, requestedAt: formatter.date(from: "2026-09-07T03:59:59Z")!,
                                           provider: "deepseek-official", model: "deepseek-v4-pro", input: 1_000_000, cachedInput: 1_000_000, output: 1_000_000)
        XCTAssertEqual(DeepSeekPricing.estimate(peak, currency: "CNY"), Decimal(string: "36.3"))
    }

    func testProviderMergesMoneySessionsAndChildUsageWithoutInventingQuota() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let root = dir.appendingPathComponent("sessions")
        let file = try logFile(root, id: "main")
        try ([header(), model(seq: 0), usage(seq: 1)].joined(separator: "\n") + "\n").write(to: file, atomically: true, encoding: .utf8)
        let child = try logFile(root, id: "child")
        try ([header(id: "child", seed: 2, child: true), model(seq: 0), usage(seq: 1), usage(seq: 2, input: 10, output: 2)].joined(separator: "\n") + "\n")
            .write(to: child, atomically: true, encoding: .utf8)
        actor Counter { var calls = 0; func increment() { calls += 1 } }
        let counter = Counter(), now = now
        let balance = try JSONDecoder().decode(DeepSeekBalance.self, from: Data(Self.balanceJSON.utf8))
        let provider = DeepSeekUsageProvider(directory: dir, transcripts: DeepSeekTranscriptStore(root: root), readBalance: {
            await counter.increment(); return balance
        }, clock: { now })
        let report = try await provider.fetchUsage(agents: [], historyHours: 169)
        _ = try await provider.fetchUsage(agents: [], historyHours: 169)
        let calls = await counter.calls
        XCTAssertEqual(calls, 1)
        XCTAssertEqual(report.sessions.count, 1)
        XCTAssertEqual(report.sessions[0].tokensIn, 8086)
        XCTAssertEqual(report.consumption.reduce(0) { $0 + $1.tokensIn }, 8096)
        XCTAssertTrue(report.snapshots.isEmpty)
        XCTAssertTrue(report.history.isEmpty)
        XCTAssertTrue(report.consumerIdsByQuota.isEmpty)
        XCTAssertEqual(report.discoveredAgents[0].source, L10n.sourceDeepSeekSessions)
        XCTAssertEqual(report.discoveredAgents.map(\.id), report.consumers.map(\.id))
        XCTAssertTrue(report.discoveredAgents.allSatisfy { $0.id.hasPrefix("deepseek-model:") && $0.isAPIBilled })
        XCTAssertEqual(report.billing[0].balances[0].total, Decimal(string: "8.85"))
        XCTAssertEqual(report.billing[0].estimatedCost(currency: "CNY"), Decimal(string: "0.012414"))
        let combined = try await CombinedUsageProvider([.init("DeepSeek", provider)]).fetchUsage(agents: [], historyHours: 169)
        XCTAssertEqual(combined.billing, report.billing)
    }

    func testBalanceFailureKeepsLocalDataAndUnpricedCostsStayUnknown() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let root = dir.appendingPathComponent("sessions"), now = now
        let file = try logFile(root, id: "main")
        try ([header(), model(seq: 0, name: "future-model"), usage(seq: 1)].joined(separator: "\n") + "\n").write(to: file, atomically: true, encoding: .utf8)
        let provider = DeepSeekUsageProvider(directory: dir, transcripts: DeepSeekTranscriptStore(root: root),
                                            readBalance: { throw UsageProviderError("offline") }, clock: { now })
        let report = try await provider.fetchUsage(agents: [], historyHours: 48)
        XCTAssertEqual(report.sessions.count, 1)
        XCTAssertEqual(report.sourceNotices["DeepSeek"], "offline")
        XCTAssertTrue(report.billing[0].balances.isEmpty)
        XCTAssertNil(report.billing[0].estimatedCost(currency: "CNY"))
        XCTAssertEqual(report.billing[0].estimatedCost(currency: "CNY", during: DateInterval(start: now.addingTimeInterval(1), duration: 10)), 0)
    }

    func testFreshInstallDetectionAndDSHHomeOverride() throws {
        let home = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: home) }
        XCTAssertEqual(DeepSeekLocator.dataDirectory(environment: [:], home: home), home.appendingPathComponent(".dsh", isDirectory: true))
        XCTAssertEqual(DeepSeekLocator.dataDirectory(environment: ["DSH_HOME": "  "], home: home), home.appendingPathComponent(".dsh", isDirectory: true))
        XCTAssertEqual(DeepSeekLocator.dataDirectory(environment: ["DSH_HOME": "~/custom"], home: home).path, home.appendingPathComponent("custom").path)
        XCTAssertFalse(DeepSeekLocator.isInstalled(directory: home))
        try FileManager.default.createDirectory(at: home.appendingPathComponent("profiles"), withIntermediateDirectories: true)
        XCTAssertTrue(DeepSeekLocator.isInstalled(directory: home))
        XCTAssertTrue(SourceStatus(id: "deepseek", name: "DeepSeek", detail: "", state: .ready(plan: nil)).isReady)
    }

    private func temporaryDirectory() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agenthud-deepseek-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
    private func logFile(_ root: URL, id: String) throws -> URL {
        let dir = root.appendingPathComponent("project/\(id)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("session.jsonl")
    }
    private func json(_ object: [String: Any]) -> String {
        String(decoding: try! JSONSerialization.data(withJSONObject: object, options: .sortedKeys), as: UTF8.self)
    }
    private func header(id: String = "main", seed: Int? = nil, child: Bool = false) -> String {
        var value: [String: Any] = ["type": "session", "version": 0, "id": id, "createdAt": now.addingTimeInterval(-10).timeIntervalSince1970 * 1000, "cwd": "/test/project", "delegationDepth": child ? 1 : 0]
        if let seed { value["seedLength"] = seed }
        if child { value["origin"] = "subagent" }
        return json(value)
    }
    private func event(_ type: String, seq: Int, data: [String: Any]) -> String {
        json(["type": type, "seq": seq, "time": now.timeIntervalSince1970 * 1000, "data": data])
    }
    private func model(seq: Int, name: String = "deepseek-v4-flash") -> String {
        event("request/header", seq: seq, data: ["header": ["config": ["provider": "deepseek-official", "model": name]], "reason": "initial"])
    }
    private func usage(seq: Int, input: Int = 8086, output: Int = 58, cached: Int = 0, chunk: Bool = false) -> String {
        let counts = ["inputTokens": input, "outputTokens": output, "cacheReadTokens": cached, "reasoningTokens": 33]
        var data: [String: Any] = ["turn": 1, "step": 1]
        if chunk { data["chunk"] = ["type": "usage", "usage": counts] } else { data["usage"] = counts }
        return event(chunk ? "assistant/chunk" : "assistant/message", seq: seq, data: data)
    }
    private func feed(_ transcript: inout DeepSeekTranscript, _ line: String) throws { try transcript.ingest(Data(line.utf8)) }
    private func append(_ text: String, to file: URL) throws {
        let handle = try FileHandle(forWritingTo: file)
        defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }
}
