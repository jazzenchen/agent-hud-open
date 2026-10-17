import Foundation
import XCTest
@testable import AgentHUDCore

final class ClaudeSessionNavigationTests: XCTestCase, @unchecked Sendable {
    func testCodeAndCoworkOriginsCannotBorrowTheOtherDesktopRouteBeforeMetadataRefresh() async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let projects = base.appendingPathComponent("projects"), project = projects.appendingPathComponent("same-cwd")
        let code = base.appendingPathComponent("code"), cowork = base.appendingPathComponent("cowork"), origins = base.appendingPathComponent("origins")
        for folder in [project, code.appendingPathComponent("org/account"), cowork.appendingPathComponent("org/account")] {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        defer { try? FileManager.default.removeItem(at: base) }
        let now = Date(), iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        let cases: [(String, String, URL, ClaudeSessionOrigins.Host)] = [("only-code", "local-agent", code, .cowork),
                                                                       ("only-cowork", "claude-desktop", cowork, .desktop)]
        for (id, entrypoint, root, _) in cases {
            let prompt = #"{"sessionId":"\#(id)","cwd":"/same/workspace","type":"user","message":{"role":"user","content":"Task"},"timestamp":"\#(iso.format(now.addingTimeInterval(-61)))","entrypoint":"\#(entrypoint)"}"#
            let line = #"{"sessionId":"\#(id)","cwd":"/same/workspace","type":"assistant","message":{"id":"\#(id)-msg","role":"assistant","model":"claude-sonnet-4-6","content":[{"type":"text","text":"done"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}},"timestamp":"\#(iso.format(now.addingTimeInterval(-60)))","entrypoint":"\#(entrypoint)"}"#
            try (prompt + "\n" + line + "\n").write(to: project.appendingPathComponent(id + ".jsonl"), atomically: true, encoding: .utf8)
            let local = "local_" + UUID().uuidString
            try JSONSerialization.data(withJSONObject: ["sessionId": local, "cliSessionId": id, "isArchived": false])
                .write(to: root.appendingPathComponent("org/account/" + local + ".json"))
        }
        let provider = ClaudeCodeProvider(engine: nil, transcripts: ClaudeTranscriptStore(root: projects), history: QuotaHistoryStore(),
                                          sessionOriginsDirectory: origins, desktopSessions: ClaudeDesktopSessions(codeRoot: code, coworkRoot: cowork), clock: { now })
        var report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.count, 2)
        XCTAssertEqual(report.completions.count, 2)
        XCTAssertTrue(report.sessions.allSatisfy { $0.navigationTarget == nil }, "a known entrypoint cannot use the other Desktop metadata family")
        for (id, _, _, host) in cases {
            let payload = try JSONSerialization.data(withJSONObject: ["session_id": id, "hook_event_name": "SessionStart", "source": "resume"])
            try ClaudeSessionOrigins.record(data: payload, target: nil, now: now, directory: origins, host: host)
        }
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertTrue(report.sessions.allSatisfy { $0.navigationTarget == nil }, "a newer host hook waits for matching metadata instead of opening the prior lane")
        XCTAssertTrue(report.completions.allSatisfy { $0.navigationTarget == nil })
    }

    func testDesktopCodeAndCoworkMappingsRespectLaterExplicitOriginsForAllCompletions() async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let projects = base.appendingPathComponent("projects"), project = projects.appendingPathComponent("same-cwd")
        let code = base.appendingPathComponent("code"), cowork = base.appendingPathComponent("cowork"), origins = base.appendingPathComponent("origins")
        for folder in [project, code.appendingPathComponent("org/account"), cowork.appendingPathComponent("org/account")] {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        defer { try? FileManager.default.removeItem(at: base) }
        let now = Date(), iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        let codeID = "local_" + UUID().uuidString, coworkID = "local_" + UUID().uuidString
        for (id, entrypoint) in [("code-cli", "claude-desktop"), ("cowork-cli", "cli")] {
            let lines = [
                #"{"sessionId":"\#(id)","cwd":"/same/workspace","type":"user","message":{"role":"user","content":"Task"},"timestamp":"\#(iso.format(now.addingTimeInterval(-60)))","entrypoint":"\#(entrypoint)"}"#,
                #"{"sessionId":"\#(id)","cwd":"/same/workspace","type":"assistant","message":{"id":"\#(id)-msg","role":"assistant","model":"claude-sonnet-4-6","content":[{"type":"text","text":"done"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}},"timestamp":"\#(iso.format(now.addingTimeInterval(-59)))","entrypoint":"\#(entrypoint)"}"#
            ]
            try (lines.joined(separator: "\n") + "\n").write(to: project.appendingPathComponent(id + ".jsonl"), atomically: true, encoding: .utf8)
        }
        let codeMetadata = code.appendingPathComponent("org/account/" + codeID + ".json")
        try JSONSerialization.data(withJSONObject: ["sessionId": codeID, "cliSessionId": "code-cli", "isArchived": false]).write(to: codeMetadata)
        try JSONSerialization.data(withJSONObject: ["sessionId": coworkID, "cliSessionId": "cowork-cli", "isArchived": false])
            .write(to: cowork.appendingPathComponent("org/account/" + coworkID + ".json"))
        let provider = ClaudeCodeProvider(engine: nil, transcripts: ClaudeTranscriptStore(root: projects), history: QuotaHistoryStore(),
                                          sessionOriginsDirectory: origins, desktopSessions: ClaudeDesktopSessions(codeRoot: code, coworkRoot: cowork), clock: { now })
        XCTAssertTrue(provider.watchedDirectories?.contains(code) == true)
        XCTAssertTrue(provider.watchedDirectories?.contains(cowork) == true)
        func origin(_ id: String, host: ClaudeSessionOrigins.Host, target: SessionNavigationTarget? = nil, after: TimeInterval) throws {
            let payload = try JSONSerialization.data(withJSONObject: ["session_id": id, "hook_event_name": "SessionStart", "source": "resume"])
            try ClaudeSessionOrigins.record(data: payload, target: target, now: now.addingTimeInterval(after), directory: origins, host: host)
        }
        var report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .claudeDesktopSession(id: codeID))
        XCTAssertNil(report.sessions.first { $0.id == "cowork-cli" }?.navigationTarget, "a CLI-labelled transcript cannot borrow an old Desktop mapping")
        let completionIDs = Set(report.completions.map(\.id))
        try origin("cowork-cli", host: .cowork, after: 0)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "cowork-cli" }?.navigationTarget, .claudeCoworkSession(id: coworkID))
        XCTAssertEqual(report.completions.first { $0.sessionID == "cowork-cli" }?.navigationTarget, .claudeCoworkSession(id: coworkID))
        for host in [ClaudeSessionOrigins.Host.cli, .unsupported] {
            try origin("code-cli", host: host, after: host == .cli ? 1 : 2)
            report = try await provider.fetchUsage(agents: [], historyHours: 24)
            XCTAssertNil(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, "a later headless/unsupported source clears Desktop")
            XCTAssertNil(report.completions.first { $0.sessionID == "code-cli" }?.navigationTarget)
        }
        try origin("code-cli", host: .cli, target: .iTermSession(id: "pane-A"), after: 3)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .iTermSession(id: "pane-A"))
        XCTAssertEqual(report.completions.first { $0.sessionID == "code-cli" }?.navigationTarget, .iTermSession(id: "pane-A"))
        try origin("code-cli", host: .desktop, after: 4)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .claudeDesktopSession(id: codeID))
        try JSONSerialization.data(withJSONObject: ["sessionId": codeID, "cliSessionId": "code-cli", "isArchived": true]).write(to: codeMetadata, options: .atomic)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.map(\.id), ["cowork-cli"], "a session the desktop app archived leaves the list")
        XCTAssertNil(report.completions.first { $0.sessionID == "code-cli" }?.navigationTarget)
        try JSONSerialization.data(withJSONObject: ["sessionId": codeID, "cliSessionId": "code-cli", "isArchived": false]).write(to: codeMetadata, options: .atomic)
        func newerTranscript(_ entrypoint: String, after: TimeInterval) throws {
            let line = #"{"sessionId":"code-cli","cwd":"/same/workspace","type":"user","message":{"role":"user","content":"Continue"},"timestamp":"\#(iso.format(now.addingTimeInterval(after)))","entrypoint":"\#(entrypoint)"}"#
            let handle = try FileHandle(forWritingTo: project.appendingPathComponent("code-cli.jsonl"))
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: Data((line + "\n").utf8))
        }
        try newerTranscript("sdk-cli", after: 5)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, "newer headless transcript clears an old Desktop origin when hooks are disabled")
        XCTAssertNil(report.completions.first { $0.sessionID == "code-cli" }?.navigationTarget)
        try origin("code-cli", host: .desktop, after: 6)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .claudeDesktopSession(id: codeID),
                       "a fresh Desktop hook wins before the older SDK transcript is refreshed")
        try newerTranscript("cli", after: 7)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, "a newer CLI surface cannot borrow Desktop or guess a terminal")
        try origin("code-cli", host: .cli, target: .iTermSession(id: "pane-B"), after: 8)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .iTermSession(id: "pane-B"))
        try newerTranscript("claude-desktop", after: 9)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, .claudeDesktopSession(id: codeID),
                       "newer Desktop transcript uses exact metadata instead of a stale terminal origin")
        try newerTranscript("sdk-cli", after: 10)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(report.sessions.first { $0.id == "code-cli" }?.navigationTarget, "newer SDK transcript also clears an interactive origin of the same CLI host")
        XCTAssertNil(report.completions.first { $0.sessionID == "code-cli" }?.navigationTarget)
        XCTAssertEqual(Set(report.completions.map(\.id)), completionIDs, "metadata never produces or duplicates completion events")
    }

    func testIndependentSessionTargetsResumeAndClearEveryCachedCompletion() async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let projects = base.appendingPathComponent("projects"), project = projects.appendingPathComponent("same-cwd")
        let origins = base.appendingPathComponent("origins")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        let now = Date(), iso = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        for session in ["session-A", "session-B"] {
            var lines: [String] = []
            for turn in 0..<2 {
                let ago = Double(60 - turn * 10)
                lines.append(#"{"sessionId":"\#(session)","cwd":"/same/workspace","type":"user","message":{"role":"user","content":"Task \#(turn)"},"timestamp":"\#(iso.format(now.addingTimeInterval(-ago)))","entrypoint":"cli"}"#)
                lines.append(#"{"sessionId":"\#(session)","cwd":"/same/workspace","type":"assistant","message":{"id":"\#(session)-msg-\#(turn)","role":"assistant","model":"claude-sonnet-4-6","content":[{"type":"text","text":"done"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}},"timestamp":"\#(iso.format(now.addingTimeInterval(-ago + 1)))","entrypoint":"cli"}"#)
            }
            try (lines.joined(separator: "\n") + "\n").write(to: project.appendingPathComponent(session + ".jsonl"), atomically: true, encoding: .utf8)
        }
        let provider = ClaudeCodeProvider(engine: nil, transcripts: ClaudeTranscriptStore(root: projects), history: QuotaHistoryStore(),
                                          sessionOriginsDirectory: origins, clock: { now })
        XCTAssertTrue(provider.watchedDirectories?.contains(origins) == true)
        func record(_ session: String, _ target: SessionNavigationTarget?, after: TimeInterval) throws {
            let payload = try JSONSerialization.data(withJSONObject: ["session_id": session, "hook_event_name": "SessionStart", "source": "resume"])
            try ClaudeSessionOrigins.record(data: payload, target: target, now: now.addingTimeInterval(after), directory: origins)
        }
        var report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.count, 2)
        XCTAssertEqual(report.completions.count, 4)
        XCTAssertTrue(report.sessions.allSatisfy { $0.navigationTarget == nil })
        try record("session-A", .iTermSession(id: "pane-A"), after: 0)
        try record("session-B", .iTermSession(id: "pane-B"), after: 0)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        for session in report.sessions {
            let expected = SessionNavigationTarget.iTermSession(id: session.id == "session-A" ? "pane-A" : "pane-B")
            XCTAssertEqual(session.navigationTarget, expected)
            XCTAssertTrue(report.completions.filter { $0.sessionID == session.id }.allSatisfy { $0.navigationTarget == expected })
        }
        let ids = Set(report.completions.map(\.id))
        try record("session-A", .iTermSession(id: "new-pane-A"), after: 1)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertEqual(report.sessions.first { $0.id == "session-A" }?.navigationTarget, .iTermSession(id: "new-pane-A"))
        XCTAssertTrue(report.completions.filter { $0.sessionID == "session-A" }.allSatisfy { $0.navigationTarget == .iTermSession(id: "new-pane-A") })
        try record("session-A", nil, after: 2)
        report = try await provider.fetchUsage(agents: [], historyHours: 24)
        XCTAssertNil(report.sessions.first { $0.id == "session-A" }?.navigationTarget)
        XCTAssertTrue(report.completions.filter { $0.sessionID == "session-A" }.allSatisfy { $0.navigationTarget == nil })
        XCTAssertEqual(report.sessions.first { $0.id == "session-B" }?.navigationTarget, .iTermSession(id: "pane-B"))
        XCTAssertEqual(Set(report.completions.map(\.id)), ids, "origin callbacks neither invent nor duplicate completed turns")
    }
}
