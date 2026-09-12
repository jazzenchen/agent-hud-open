import XCTest
@testable import AgentHUDCore

final class PiSessionObserverTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1_789_000_000)

    private func temporaryHome() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("PiObserverTests-\(UUID())")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url.resolvingSymlinksInPath()
    }

    private func observation(_ state: SessionTurn.State, age: TimeInterval = 0, turnID: String = "turn") -> PiSessionObserver.Observation {
        .init(version: 1, sessionID: "pi:session", sessionFile: nil, workspace: "/workspace", title: "Pi task",
              model: "model", providerID: "provider", turnID: turnID, state: state,
              startedAtMs: Int64(now.addingTimeInterval(-300 - age).timeIntervalSince1970 * 1000),
              observedAtMs: Int64(now.addingTimeInterval(-age).timeIntervalSince1970 * 1000))
    }

    private func write(_ data: Data, to file: URL) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: file, options: .atomic)
    }

    private func report(_ local: OpenAgentLocalStore.Result) async throws -> UsageReport {
        let now = now
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in local },
            fetchQuota: { _, _ in throw ProviderFailure.format }, history: QuotaHistoryStore(fileURL: nil), clock: { now })
        return try await provider.fetchUsage(agents: [], historyHours: 168)
    }

    func testInstallHonorsCustomHomePreservesOtherExtensionsAndIsReversible() throws {
        let home = try temporaryHome(), custom = home.appendingPathComponent("custom-agent")
        let env = ["PI_CODING_AGENT_DIR": custom.path]
        let other = custom.appendingPathComponent("extensions/other.ts")
        try write(Data("other extension".utf8), to: other)
        for _ in 0..<2 { try PiSessionObserver.configure(enabled: true, home: home, environment: env) }
        XCTAssertTrue(PiSessionObserver.isInstalled(home: home, environment: env))
        XCTAssertFalse(PiSessionObserver.isInstalled(home: home, environment: [:]))
        XCTAssertEqual(try String(contentsOf: other, encoding: .utf8), "other extension")
        try PiSessionObserver.configure(enabled: false, home: home, environment: env)
        XCTAssertFalse(PiSessionObserver.isInstalled(home: home, environment: env))
        XCTAssertTrue(FileManager.default.fileExists(atPath: other.path))
        let occupied = custom.appendingPathComponent("extensions/agent-hud.ts")
        try write(Data("unrelated".utf8), to: occupied)
        XCTAssertThrowsError(try PiSessionObserver.configure(enabled: true, home: home, environment: env))
        XCTAssertThrowsError(try PiSessionObserver.configure(enabled: false, home: home, environment: env))
        XCTAssertEqual(try String(contentsOf: occupied, encoding: .utf8), "unrelated")
    }

    func testAutomaticSetupOnlyInstallsForAnExistingPiDirectory() throws {
        let home = try temporaryHome(), custom = home.appendingPathComponent("custom-agent")
        let env = ["PI_CODING_AGENT_DIR": custom.path]
        try PiSessionObserver.configureIfAvailable(home: home, environment: env)
        XCTAssertFalse(FileManager.default.fileExists(atPath: custom.path))
        try FileManager.default.createDirectory(at: custom, withIntermediateDirectories: true)
        try PiSessionObserver.configureIfAvailable(home: home, environment: env)
        XCTAssertTrue(PiSessionObserver.isInstalled(home: home, environment: env))
    }

    func testRunningBeforeFirstResponseThenUsageAndCompletionMergeIntoOneSession() async throws {
        let home = try temporaryHome(), paths = OpenAgentPaths(home: home, environment: [:])
        let observerFile = paths.piTurns.appendingPathComponent("turn.json")
        try write(JSONEncoder().encode(observation(.running)), to: observerFile)
        let store = OpenAgentLocalStore(paths: paths)
        let local = await store.index(since: now.addingTimeInterval(-86400))
        let running = try await report(local)
        XCTAssertEqual(running.sessions.count, 1)
        XCTAssertTrue(try XCTUnwrap(running.sessions.first).isLive)
        XCTAssertEqual(running.consumers.first?.model, "model · provider")
        XCTAssertEqual(running.turns.first?.state, .running)
        XCTAssertTrue(running.consumption.isEmpty)

        let transcript = paths.pi.appendingPathComponent("sessions/workspace/session.jsonl")
        try write(Data("""
        {"type":"session","id":"session","cwd":"/workspace","timestamp":"\(now.addingTimeInterval(-600).ISO8601Format())"}
        {"type":"message","id":"response","timestamp":"\(now.addingTimeInterval(-10).ISO8601Format())","message":{"role":"assistant","model":"model","provider":"provider","usage":{"input":10,"output":5,"cacheRead":20,"cacheWrite":2},"stopReason":"stop"}}
        """.utf8), to: transcript)
        try write(JSONEncoder().encode(observation(.completed)), to: observerFile)
        let completed = try await report(await store.index(since: now.addingTimeInterval(-86400)))
        XCTAssertEqual(completed.sessions.count, 1)
        let session = try XCTUnwrap(completed.sessions.first)
        XCTAssertFalse(session.isLive)
        XCTAssertEqual(session.tokensIn, 12)
        XCTAssertEqual(session.tokensOut, 5)
        XCTAssertEqual(session.cacheReadTokens, 20)
        XCTAssertEqual(session.task, "Pi task")
        XCTAssertEqual(URL(fileURLWithPath: try XCTUnwrap(session.transcriptPath)).resolvingSymlinksInPath(), transcript.resolvingSymlinksInPath())
        XCTAssertEqual(completed.consumption.count, 1)
        XCTAssertEqual(completed.completions.count, 1)
        XCTAssertEqual(completed.turns.first?.state, .completed)
        let cached = try await report(await store.index(since: now.addingTimeInterval(-86400)))
        XCTAssertEqual(cached.consumption, completed.consumption)
        XCTAssertEqual(cached.turns, completed.turns, "A poll must not manufacture a newer source timestamp")
    }

    func testErrorsStaleRunsAndHistoricalCompletionDoNotFinishNewRun() async throws {
        var running = observation(.running).session
        running.turns.insert(observation(.completed, age: 600, turnID: "old").turn, at: 0)
        let active = try await report(.init(sessions: [running]))
        XCTAssertTrue(try XCTUnwrap(active.sessions.first).isLive)
        for value in [observation(.ended), observation(.running, age: 121)] {
            let result = try await report(.init(sessions: [value.session]))
            XCTAssertFalse(try XCTUnwrap(result.sessions.first).isLive)
            XCTAssertTrue(result.completions.isEmpty)
            XCTAssertEqual(result.turns.last?.state, .ended)
        }
        XCTAssertThrowsError(try PiSessionObserver.read(Data("{}".utf8)))
    }

    func testNativeExtensionLifecycleRetriesHeartbeatAndPrivacy() throws {
        let home = try temporaryHome()
        let script = home.appendingPathComponent("observer.mjs"), test = home.appendingPathComponent("test.mjs")
        try write(Data(PiSessionObserver.script.utf8), to: script)
        try write(Data(#"""
        import assert from 'node:assert/strict';
        import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
        import { join } from 'node:path';
        import observer from './observer.mjs';
        process.env.PI_CODING_AGENT_DIR = process.argv[2];
        let timer, cleared = 0, clock = 1789000000000;
        Date.now = () => ++clock;
        globalThis.setInterval = (fn, ms) => { assert.equal(ms, 15000); timer = fn; return { unref() {} }; };
        globalThis.clearInterval = () => { cleared++; };
        const handlers = new Map();
        const pi = { on: (name, handler) => handlers.set(name, handler) };
        const ctx = { cwd: '/workspace', model: { id: 'model', provider: 'provider' },
          sessionManager: { getSessionId: () => 'session', getSessionFile: () => undefined, getSessionName: () => 'Task' } };
        const emit = (name, event = {}) => handlers.get(name)?.(event, ctx);
        const directory = join(process.argv[2], 'agent-hud', 'turns');
        const rows = () => readdirSync(directory).filter(f => f.endsWith('.json')).map(f => JSON.parse(readFileSync(join(directory, f), 'utf8')));
        observer(pi);
        emit('session_start');
        emit('agent_start');
        const first = rows()[0];
        assert.equal(first.state, 'running');
        assert.equal(first.sessionFile, undefined);
        emit('message_end', { message: { role: 'assistant', stopReason: 'error', content: 'private prompt', usage: { input: 99 } } });
        emit('agent_end');
        emit('agent_start'); // retry, still the same logical turn
        assert.equal(rows()[0].turnID, first.turnID);
        assert.equal(rows()[0].state, 'running');
        timer();
        assert.ok(rows()[0].observedAtMs > first.observedAtMs);
        emit('message_end', { message: { role: 'assistant', stopReason: 'stop' } });
        emit('agent_end'); // queued follow-up must not trigger completion
        assert.equal(rows()[0].state, 'running');
        emit('agent_settled');
        assert.equal(rows()[0].state, 'completed');
        const completedAt = rows()[0].observedAtMs;
        emit('session_shutdown');
        assert.equal(rows()[0].observedAtMs, completedAt);
        for (const reason of ['error', 'aborted', 'length']) {
          emit('agent_start');
          emit('message_end', { message: { role: 'assistant', stopReason: reason } });
          emit('agent_settled');
        }
        emit('agent_start');
        emit('session_shutdown');
        assert.equal(rows().filter(r => r.state === 'completed').length, 1);
        assert.equal(rows().filter(r => r.state === 'ended').length, 4);
        assert.ok(cleared >= 5);
        assert.equal(JSON.stringify(rows()).includes('private prompt'), false);
        assert.equal(JSON.stringify(rows()).includes('usage'), false);
        assert.equal(JSON.stringify(rows()).includes('content'), false);
        // An unwritable inbox cannot break an agent run.
        process.env.PI_CODING_AGENT_DIR = join(process.argv[2], 'blocked');
        writeFileSync(process.env.PI_CODING_AGENT_DIR, 'file instead of directory');
        observer(pi);
        emit('agent_start'); emit('agent_settled');
        console.log('Pi native lifecycle checks passed');
        """#.utf8), to: test)
        let output = try ProviderCommand.run("/usr/bin/env", ["node", test.path, home.path])
        XCTAssertTrue(output.contains("Pi native lifecycle checks passed"), output)
    }
}
