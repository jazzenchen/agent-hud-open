import AgentHUDSupport
import Foundation
import SQLite3
import XCTest
@testable import AgentHUDCore

final class OpenClawProviderTests: XCTestCase, @unchecked Sendable {
    private let now = Date()
    private var ms: Int64 { RecordCoding.milliseconds(now) }
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }
    private func sql(_ db: OpaquePointer, _ query: String) throws {
        guard sqlite3_exec(db, query, nil, nil, nil) == SQLITE_OK else { throw ProviderFailure.local }
    }
    private func quoted(_ text: String) -> String { "'" + text.replacingOccurrences(of: "'", with: "''") + "'" }
    private func assistant(_ id: String, provider: String = "anthropic", model: String = "claude-test", response: String? = "r-\(UUID().uuidString)",
                           extra: String = "", usage: String = #"{"input":100,"output":20,"cacheRead":50,"cacheWrite":10,"cost":{"total":0.5}}"#, at: Int64? = nil) -> String {
        let response = response.map { #","responseId":"\#($0)""# } ?? ""
        return #"{"type":"message","id":"\#(id)","parentId":null,"timestamp":"2026-09-14T00:00:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"secret prompt"}],"api":"anthropic-messages","provider":"\#(provider)","model":"\#(model)"\#(response)\#(extra),"usage":\#(usage),"stopReason":"stop","timestamp":\#(at ?? ms)}}"#
    }

    /// Only the session columns the parser reads. `auth_profile_store` is a view over a dropped table, so any query that touched it would fail.
    private func agentDatabase(_ root: URL, windows: [(id: String, status: String?, node: String?, spawnedBy: String?)] = [("s1", nil, nil, nil)],
                               events: [(session: String, json: String, created: Int64?)]) throws -> URL {
        let url = root.appendingPathComponent("agents/main/agent/openclaw-agent.sqlite")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        var handle: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &handle), SQLITE_OK)
        let db = try XCTUnwrap(handle)
        defer { sqlite3_close(db) }
        try sql(db, """
            PRAGMA journal_mode=WAL;
            CREATE TABLE credentials (value TEXT); CREATE VIEW auth_profile_store AS SELECT value FROM credentials; DROP TABLE credentials;
            CREATE TABLE session_nodes (session_key TEXT PRIMARY KEY, current_session_id TEXT NOT NULL, entry_json TEXT NOT NULL, label TEXT, display_name TEXT, spawned_by TEXT) STRICT;
            CREATE TABLE session_windows (session_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, transcript_updated_at INTEGER,
                started_at INTEGER, ended_at INTEGER, status TEXT, spawned_by TEXT, display_name TEXT) STRICT;
            CREATE TABLE transcript_events (session_id TEXT NOT NULL, seq INTEGER NOT NULL, event_json TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY (session_id, seq)) STRICT;
            """)
        for window in windows {
            try sql(db, "INSERT INTO session_windows VALUES (\(quoted(window.id)), \(ms - 60000), \(ms), NULL, \(ms - 5000), \(window.status == "running" ? "NULL" : "\(ms)"), \(window.status.map(quoted) ?? "NULL"), \(window.spawnedBy.map(quoted) ?? "NULL"), 'Fixture \(window.id)')")
            if let entry = window.node {
                try sql(db, "INSERT INTO session_nodes VALUES ('agent:main:\(window.id)', \(quoted(window.id)), \(quoted(entry)), NULL, NULL, NULL)")
            }
        }
        for (index, event) in events.enumerated() {
            try sql(db, "INSERT INTO transcript_events VALUES (\(quoted(event.session)), \(index), \(quoted(event.json)), \(event.created ?? ms))")
        }
        return url
    }

    func testDatabaseCountsResponsesOnceAndSkipsBookkeepingAndDelegatedRows() throws {
        let url = try agentDatabase(try directory(), events: [
            ("s1", #"{"type":"session","version":3,"id":"s1","timestamp":"2026-09-14T00:00:00.000Z","cwd":"/work/app"}"#, nil),
            ("s1", assistant("a1", response: "resp-1"), nil),
            ("s1", assistant("a1-copy", response: "resp-1"), nil),
            ("s1", assistant("a2", response: nil, usage: #"{"input":5,"output":7}"#), nil),
            ("s1", assistant("b1", provider: "openclaw", model: "delivery-mirror", extra: #","api":"openclaw-transcript""#), nil),
            ("s1", assistant("c1", provider: "claude-cli", model: "claude-sonnet-5"), nil),
            ("s1", assistant("x1", model: "gpt-5.5", extra: #","idempotencyKey":"codex-app-server:thread:turn:assistant""#), nil),
            ("s1", assistant("z1", usage: #"{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}"#), nil),
            ("s1", assistant("old", at: ms - 20 * 86400_000), ms - 20 * 86400_000)
        ])
        let session = try XCTUnwrap(OpenClawSessions.read(url, since: now.addingTimeInterval(-86400)).sessions.first)
        XCTAssertEqual(session.id, "openclaw:s1")
        XCTAssertEqual(session.title, "Fixture s1")
        XCTAssertEqual(session.workspace, "/work/app")
        XCTAssertEqual(session.events.count, 2)
        XCTAssertEqual(session.events.map(\.input), [110, 5], "In is fresh input plus cache writes")
        XCTAssertEqual(session.events.map(\.output), [20, 7], "Reasoning is already inside output")
        XCTAssertEqual(session.events.map(\.cacheRead), [50, 0])
        XCTAssertTrue(session.events[0].id.hasPrefix("response:"))
        XCTAssertTrue(session.events[1].id.hasPrefix("entry:"))
        XCTAssertTrue(session.turns.isEmpty, "A window without Gateway lifecycle status has no turn")
    }

    func testTheLabelTheUserGaveBeatsTheDisplayName() throws {
        let url = try agentDatabase(try directory(), windows: [("s1", nil, "{}", nil)], events: [("s1", assistant("a1"), nil)])
        XCTAssertEqual(try OpenClawSessions.read(url, since: now.addingTimeInterval(-86400)).sessions.first?.title, "Fixture s1")
        var handle: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &handle), SQLITE_OK)
        try sql(try XCTUnwrap(handle), "UPDATE session_nodes SET label = 'Release notes', display_name = 'Generated title'")
        sqlite3_close(handle)
        XCTAssertEqual(try OpenClawSessions.read(url, since: now.addingTimeInterval(-86400)).sessions.first?.title, "Release notes")
    }

    func testGatewayStatusMapsToTurnsAndOnlyDoneCompletes() throws {
        let url = try agentDatabase(try directory(), windows: [
            ("run", "running", #"{"lifecycleRunId":"run-a"}"#, nil),
            ("done", "done", #"{"lastRunId":"run-b"}"#, nil),
            ("fail", "failed", #"{"lastRunId":"run-c"}"#, nil),
            ("child", "done", #"{"lastRunId":"run-d"}"#, "agent:main:done"),
            ("stale", "running", nil, nil)
        ], events: [("done", assistant("a1", model: "claude-done"), nil)])
        let sessions = Dictionary(uniqueKeysWithValues: try OpenClawSessions.read(url, since: now.addingTimeInterval(-86400)).sessions.map { ($0.id, $0) })
        XCTAssertEqual(sessions["openclaw:run"]?.turns.map(\.state), [.running])
        XCTAssertEqual(sessions["openclaw:run"]?.turns.first?.turnID, "run-a")
        XCTAssertEqual(sessions["openclaw:run"]?.turns.first?.observedAtMs, ms)
        XCTAssertEqual(sessions["openclaw:done"]?.turns.map(\.state), [.completed])
        let completion = try XCTUnwrap(sessions["openclaw:done"]?.completions.first)
        XCTAssertEqual(completion.sessionID, "openclaw:done")
        XCTAssertEqual(completion.id, SessionCompletion(sessionID: "openclaw:done", vendor: "OpenClaw", turnID: "run-b", task: "", model: "", startedAt: nil, completedAt: now).id)
        XCTAssertEqual(completion.model, "claude-done")
        XCTAssertEqual(sessions["openclaw:fail"]?.turns.map(\.state), [.ended])
        XCTAssertEqual(sessions["openclaw:done"]?.turns.map(\.outcome), [.completed])
        XCTAssertEqual(sessions["openclaw:fail"]?.turns.map(\.outcome), [nil], "a status that only says the run did not complete gives no reason")
        XCTAssertEqual(sessions["openclaw:fail"]?.completions.isEmpty, true)
        XCTAssertEqual(sessions["openclaw:child"]?.turns.isEmpty, true, "Sub-agents never finish a turn")
        XCTAssertEqual(sessions["openclaw:stale"]?.turns.isEmpty, true, "Only a session's current window carries lifecycle status")
    }

    func testLocalStoreScansAgentsAndMergesTranscriptCopiesIntoDatabaseSession() async throws {
        let root = try directory()
        let copy = assistant("a1", response: "resp-1")
        _ = try agentDatabase(root, windows: [("s1", "done", #"{"lastRunId":"run"}"#, nil)], events: [("s1", copy, nil)])
        let sessions = root.appendingPathComponent("agents/main/sessions"), codex = root.appendingPathComponent("agents/main/agent/codex-home/sessions")
        for folder in [sessions, codex] { try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true) }
        try (copy + "\n").write(to: sessions.appendingPathComponent("s1.jsonl"), atomically: true, encoding: .utf8)
        try (assistant("r1", usage: #"{"input":1,"output":2}"#) + "\n").write(to: sessions.appendingPathComponent("s0.jsonl.reset.2026-09-13T00-00-00.000Z"), atomically: true, encoding: .utf8)
        try (copy + "\n").write(to: sessions.appendingPathComponent("s1.jsonl.reset.2026-09-12T00-00-00.000Z"), atomically: true, encoding: .utf8)
        try Data([0x28, 0xb5, 0x2f, 0xfd]).write(to: sessions.appendingPathComponent("s2.jsonl.deleted.2026-09-13T00-00-00.000Z.zst"))
        try (assistant("codex") + "\n").write(to: codex.appendingPathComponent("rollout-1.jsonl"), atomically: true, encoding: .utf8)
        let result = await AdditionalLocalStore(source: .openclaw, roots: [root.appendingPathComponent("agents")]).index(since: now.addingTimeInterval(-3600))
        XCTAssertNil(result.notice)
        XCTAssertEqual(result.sessions.map(\.id), ["openclaw:s0", "openclaw:s1"])
        XCTAssertEqual(result.sessions[1].events.count, 1)
        XCTAssertEqual(result.sessions[1].title, "Fixture s1")
        XCTAssertEqual(result.sessions[1].completions.count, 1)
        XCTAssertEqual(result.sessions.map(\.archived), [true, false],
                       "a session known only from a reset or deleted transcript is archived; one OpenClaw still holds is not")
    }

    func testRootsFollowStateDirectoryProfilesAndLegacyName() throws {
        let home = try directory()
        for name in [".clawdbot", ".openclaw-work", ".openclaw-", ".openclawx"] {
            try FileManager.default.createDirectory(at: home.appendingPathComponent(name), withIntermediateDirectories: true)
        }
        XCTAssertEqual(OpenClawSessions.roots(home: home, environment: [:]).map { $0.pathComponents.suffix(2).joined(separator: "/") },
                       [".clawdbot/agents", ".openclaw-work/agents"])
        try FileManager.default.createDirectory(at: home.appendingPathComponent(".openclaw"), withIntermediateDirectories: true)
        XCTAssertEqual(OpenClawSessions.roots(home: home, environment: [:]).first?.path, home.appendingPathComponent(".openclaw/agents").path)
        XCTAssertEqual(OpenClawSessions.roots(home: home, environment: ["OPENCLAW_STATE_DIR": "~/state"]).map(\.path), [home.path + "/state/agents"])
    }
}
