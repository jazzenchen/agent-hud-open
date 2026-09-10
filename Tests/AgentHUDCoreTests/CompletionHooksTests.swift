import Foundation
import XCTest
@testable import AgentHUDCore

final class CompletionHooksTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1788800000)
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }
    private func json(_ value: [String: Any]) throws -> ProviderJSON {
        try .read(JSONSerialization.data(withJSONObject: value))
    }

    func testGrokNamespacedCompletionSurvivesUsageLogPrecedence() async throws {
        let root = try directory(), session = root.appendingPathComponent("sessions/%2Ffixture/s")
        try FileManager.default.createDirectory(at: session, withIntermediateDirectories: true)
        let logs: [[String: Any]] = [
            ["method": "session/update", "params": ["sessionId": "s", "_meta": ["eventId": "user", "promptId": "p", "agentTimestampMs": 1788800000000],
                "update": ["sessionUpdate": "user_message_chunk"]]],
            ["method": "_x.ai/session/update", "params": ["sessionId": "s", "_meta": ["eventId": "done", "agentTimestampMs": 1788800002000],
                "update": ["sessionUpdate": "turn_completed", "prompt_id": "p", "stop_reason": "end_turn",
                    "usage": ["inputTokens": 100, "outputTokens": 20, "cachedReadTokens": 60, "modelUsage": ["grok-test": [:]]]]]]
        ]
        let lines = try (logs + [logs[1]]).map { String(decoding: try JSONSerialization.data(withJSONObject: $0), as: UTF8.self) }
        try (lines.joined(separator: "\n") + "\n").write(to: session.appendingPathComponent("updates.jsonl"), atomically: true, encoding: .utf8)
        try #"{"ts":"2026-09-07T16:53:21Z","sid":"s","pid":1,"msg":"shell.turn.inference_done","ctx":{"prompt_tokens":100,"completion_tokens":20,"cached_prompt_tokens":60}}"#
            .write(to: root.appendingPathComponent("unified.jsonl"), atomically: true, encoding: .utf8)
        let result = await AdditionalLocalStore(source: .grok, roots: [root]).index(since: now.addingTimeInterval(-10))
        let item = try XCTUnwrap(result.sessions.first)
        XCTAssertEqual(item.events.count, 1)
        XCTAssertEqual(item.events[0].input, 40)
        XCTAssertEqual(item.completions.count, 1)
        XCTAssertEqual(item.completions[0].model, "grok-test")
        XCTAssertEqual(item.completions[0].startedAt, now)
        XCTAssertEqual(item.turns.count, 1)
        XCTAssertEqual(item.turns[0].turnID, "p")
        XCTAssertEqual(item.turns[0].state, .completed)
    }

    func testGrokAbortedOrUnknownStopDoesNotNotify() throws {
        for reason in ["cancelled", "max_tokens", "error", ""] {
            let directory = try directory(), url = directory.appendingPathComponent("updates.jsonl")
            let payload = try json(["method": "_x.ai/session/update", "params": ["sessionId": directory.lastPathComponent,
                "_meta": ["eventId": "done", "agentTimestampMs": 1788800000000],
                "update": ["sessionUpdate": "turn_completed", "stop_reason": reason]]])
            try JSONEncoder().encode(payload).write(to: url)
            let item = try XCTUnwrap(GrokSessions.read(url).sessions.first)
            XCTAssertTrue(item.completions.isEmpty)
            XCTAssertEqual(item.turns.first?.state, .ended)
        }
    }

    func testAntigravityStopRequiresSuccessfulIdleLoop() throws {
        var payload: [String: Any] = ["conversationId": "s", "executionNum": 1, "terminationReason": "model_stop", "fullyIdle": true]
        let first = try XCTUnwrap(CompletionHooks.completion(source: .antigravity, payload: json(payload), now: now))
        payload["executionNum"] = 2
        let second = try XCTUnwrap(CompletionHooks.completion(source: .antigravity, payload: json(payload), now: now))
        XCTAssertNotEqual(first.id, second.id)
        payload["fullyIdle"] = false
        XCTAssertNil(try CompletionHooks.completion(source: .antigravity, payload: json(payload), now: now))
        payload["fullyIdle"] = true
        for reason in ["error", "max_steps_exceeded", "cancelled"] {
            payload["terminationReason"] = reason
            XCTAssertNil(try CompletionHooks.completion(source: .antigravity, payload: json(payload), now: now))
        }
        payload["terminationReason"] = "model_stop"; payload["error"] = "failure"
        XCTAssertNil(try CompletionHooks.completion(source: .antigravity, payload: json(payload), now: now))
    }



    func testInstallationPreservesOtherHooksAndCanBeRemoved() throws {
        let home = try directory(), executable = home.appendingPathComponent("Agent's HUD.app/Contents/MacOS/Agent HUD")
        for source in CompletionHooks.Source.allCases {
            let url = source.configuration(home: home)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            let original = source == .cursor ? #"{"version":1,"hooks":{"stop":[{"command":"other-command"}],"sessionStart":[]}}"#
                : #"{"other-hook":{"Stop":[{"command":"other-command"}]}}"#
            try original.write(to: url, atomically: true, encoding: .utf8)
            for _ in 0..<2 { try CompletionHooks.configure(source, enabled: true, executable: executable, home: home) }
            XCTAssertTrue(CompletionHooks.isInstalled(source, home: home))
            let installed = try String(contentsOf: url, encoding: .utf8)
            XCTAssertTrue(installed.contains("other-command"))
            XCTAssertEqual(installed.components(separatedBy: "--completion-hook").count, 2)
            try CompletionHooks.configure(source, enabled: false, executable: executable, home: home)
            XCTAssertFalse(CompletionHooks.isInstalled(source, home: home))
            XCTAssertTrue(try String(contentsOf: url, encoding: .utf8).contains("other-command"))
        }
    }
}
