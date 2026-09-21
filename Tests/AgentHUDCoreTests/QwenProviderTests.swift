import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

final class QwenProviderTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1_788_800_000)
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }
    private func json(_ object: [String: Any]) throws -> String {
        String(decoding: try JSONSerialization.data(withJSONObject: object), as: UTF8.self)
    }
    private func write(_ lines: [String], to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try (lines.joined(separator: "\n") + "\n").write(to: url, atomically: true, encoding: .utf8)
    }
    private func line(_ type: String, uuid: String, session: String = "s1", subtype: String? = nil, extra: [String: Any] = [:]) -> [String: Any] {
        var object: [String: Any] = ["uuid": uuid, "sessionId": session, "timestamp": "2026-09-08T12:00:00.000Z",
                                     "type": type, "cwd": "/Users/alice/repo", "version": "0.24.3"]
        if let subtype { object["subtype"] = subtype }
        return object.merging(extra) { $1 }
    }
    private func response(_ uuid: String, input: Int, output: Int, cached: Int = 0, thoughts: Int = 0, total: Int? = nil,
                          model: String = "qwen3-coder-plus", extra: [String: Any] = [:]) -> [String: Any] {
        line("system", uuid: uuid, subtype: "ui_telemetry", extra: ["systemPayload": ["uiEvent": [
            "event.name": "qwen-code.api_response", "event.timestamp": "2026-09-08T12:00:05.000Z", "model": model,
            "input_token_count": input, "output_token_count": output, "cached_content_token_count": cached,
            "thoughts_token_count": thoughts, "total_token_count": total ?? input + output, "response_text": "secret"]]].merging(extra) { $1 })
    }

    func testTelemetryCountsEachModelCallOnceWithCacheReadsOutOfInput() throws {
        let url = try directory().appendingPathComponent("projects/-Users-alice-repo/chats/s1.jsonl")
        try write([
            try json(line("user", uuid: "u1", extra: ["message": ["role": "user", "parts": [["text": "Fix the flaky login test\nthen run it"]]]])),
            try json(response("r1", input: 15873, output: 3827, cached: 1000, thoughts: 25)),
            // The assistant line repeats the same call's counts; telemetry is the record, so this adds nothing.
            try json(line("assistant", uuid: "a1", extra: ["model": "qwen3-coder-plus", "usageMetadata": ["promptTokenCount": 15873, "candidatesTokenCount": 3827]])),
            // A sub-agent call is only in telemetry.
            try json(response("r2", input: 200, output: 50, extra: ["forkedFrom": NSNull()])),
            // Gemini counts thoughts beside the output, which its total says.
            try json(response("r3", input: 100, output: 10, thoughts: 5, total: 115, model: "gemini-3-pro")),
            try json(line("system", uuid: "e1", subtype: "ui_telemetry", extra: ["systemPayload": ["uiEvent": [
                "event.name": "qwen-code.api_error", "error_type": "RateLimitError", "status_code": 429]]])),
            try json(response("zero", input: 0, output: 0)),
            try json(line("tool_result", uuid: "t1")),
        ], to: url)
        let result = try QwenSessions.read(url)
        XCTAssertNil(result.notice)
        let session = try XCTUnwrap(result.sessions.first)
        XCTAssertEqual(session.id, "qwen:s1")
        XCTAssertEqual(session.client, "Qwen Code")
        XCTAssertEqual(session.title, "Fix the flaky login test", "the first prompt names the session")
        XCTAssertEqual(session.workspace, "/Users/alice/repo")
        let events = Dictionary(uniqueKeysWithValues: session.events.map { ($0.id, $0) })
        XCTAssertEqual(Set(events.keys), ["r1", "r2", "r3"], "errors, empty calls and assistant copies are not usage")
        XCTAssertEqual([events["r1"]?.input, events["r1"]?.output, events["r1"]?.cacheRead], [14873, 3827, 1000],
                       "the prompt holds the cache reads and the output holds the reasoning")
        XCTAssertEqual(events["r3"]?.output, 15)
        XCTAssertEqual(events["r3"]?.model, "gemini-3-pro")
    }

    func testABranchCopyIsNotCountedAgainAndATitleWins() throws {
        let url = try directory().appendingPathComponent("projects/-Users-alice-repo/chats/s2.jsonl")
        try write([
            try json(line("user", uuid: "u1", session: "s2", extra: ["message": ["parts": [["text": "copied prompt"]]],
                                                                     "forkedFrom": ["sessionId": "s1", "messageUuid": "u1"]])),
            try json(response("r1", input: 100, output: 10).merging(["sessionId": "s2", "forkedFrom": ["sessionId": "s1", "messageUuid": "r1"]]) { $1 }),
            try json(response("r9", input: 40, output: 4).merging(["sessionId": "s2"]) { $1 }),
            try json(line("system", uuid: "c1", session: "s2", subtype: "custom_title", extra: ["systemPayload": ["customTitle": "Login fix"]])),
        ], to: url)
        let session = try XCTUnwrap(try QwenSessions.read(url).sessions.first)
        XCTAssertEqual(session.events.map(\.id), ["r9"], "the copied history was counted in the session it came from")
        XCTAssertEqual(session.title, "Login fix", "a title the session was given beats its first prompt")
    }

    func testOnlyChatTranscriptsUnderTheRuntimeFolderAreRead() throws {
        let home = URL(fileURLWithPath: "/Users/alice")
        XCTAssertEqual(QwenSessions.roots(home: home, environment: [:]), [home.appendingPathComponent(".qwen/projects")])
        XCTAssertEqual(QwenSessions.roots(home: home, environment: ["QWEN_HOME": "/q"]), [URL(fileURLWithPath: "/q/projects")])
        XCTAssertEqual(QwenSessions.roots(home: home, environment: ["QWEN_HOME": "/q", "QWEN_RUNTIME_DIR": "/r"]),
                       [URL(fileURLWithPath: "/r/projects")], "sessions follow the runtime folder, settings stay in the home")
        XCTAssertTrue(QwenSessions.accepts(URL(fileURLWithPath: "/q/projects/p/chats/s.jsonl")))
        XCTAssertTrue(QwenSessions.accepts(URL(fileURLWithPath: "/q/projects/p/chats/archive/s.jsonl")))
        XCTAssertFalse(QwenSessions.accepts(URL(fileURLWithPath: "/q/projects/p/chats/s.ledger.jsonl")))
        XCTAssertFalse(QwenSessions.accepts(URL(fileURLWithPath: "/q/tmp/abc/logs.json")))
        XCTAssertTrue(QwenSessions.skips(URL(fileURLWithPath: "/q/projects/p/subagents")),
                      "sub-agent calls are already in the parent's telemetry")
    }

    func testTheStopHookNamesThePromptAndIsWrittenInMilliseconds() throws {
        let payload = try ProviderJSON.read(Data(#"{"hook_event_name":"Stop","session_id":"s1","prompt_id":"s1########3","cwd":"/Users/alice/repo","last_assistant_message":"Done"}"#.utf8))
        let event = try XCTUnwrap(QwenHookFormat.completion(payload, now: now))
        XCTAssertEqual(event.turn, "s1########3")
        XCTAssertEqual(event.workspace, "/Users/alice/repo")
        let older = try ProviderJSON.read(Data(#"{"hook_event_name":"Stop","session_id":"s1"}"#.utf8))
        XCTAssertEqual(QwenHookFormat.completion(older, now: now)?.turn, "stop-\(RecordCoding.milliseconds(now))")
        XCTAssertNil(QwenHookFormat.completion(try ProviderJSON.read(Data(#"{"hook_event_name":"StopFailure","session_id":"s1"}"#.utf8)), now: now))

        let home = try directory()
        let settings = home.appendingPathComponent(".qwen/settings.json")
        try FileManager.default.createDirectory(at: settings.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"{"$version":4,"security":{"auth":{"selectedType":"openai"}}}"#.utf8).write(to: settings)
        try CompletionHooks.configure(.qwen, enabled: true, executable: URL(fileURLWithPath: "/tmp/hud"), home: home)
        let written = try ProviderJSON.read(Data(contentsOf: settings))
        XCTAssertEqual(written["$version"].numberValue, 4)
        XCTAssertEqual(written["security"]["auth"]["selectedType"].stringValue, "openai")
        let handler = written["hooks"]["Stop"].arrayValue?.first?["hooks"].arrayValue?.first
        XCTAssertEqual(handler?["command"].stringValue, "'/tmp/hud' --completion-hook qwen")
        XCTAssertEqual(handler?["timeout"].numberValue, 5000, "five seconds on every Qwen version")
    }
}
