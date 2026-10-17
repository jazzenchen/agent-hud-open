import XCTest
@testable import AgentHUDCore

final class OpenCodeSessionObserverTests: XCTestCase, @unchecked Sendable {
    private let now = Date(timeIntervalSince1970: 1_789_000_000)

    private func temporaryHome() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("OpenCodeObserverTests-\(UUID())")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url.resolvingSymlinksInPath()
    }

    private func write(_ data: Data, to file: URL) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: file, options: .atomic)
    }

    private func milliseconds(_ date: Date) -> Int64 { Int64(date.timeIntervalSince1970 * 1000) }

    func testThePluginGoesIntoTheGlobalPluginDirectoryAndLeavesOtherFilesAlone() throws {
        let home = try temporaryHome(), config = home.appendingPathComponent("xdg-config")
        let env = ["XDG_CONFIG_HOME": config.path]
        let plugin = config.appendingPathComponent("opencode/plugin/agent-hud.js")
        let other = config.appendingPathComponent("opencode/plugin/other.js")
        try write(Data("export const Other = async () => ({})".utf8), to: other)
        for _ in 0..<2 { try OpenCodeSessionObserver.configure(enabled: true, home: home, environment: env) }
        XCTAssertTrue(OpenCodeSessionObserver.isInstalled(home: home, environment: env))
        XCTAssertEqual(OpenCodeSessionObserver.fileState(home: home, environment: env), .installed)
        XCTAssertFalse(OpenCodeSessionObserver.isInstalled(home: home, environment: [:]))
        try OpenCodeSessionObserver.configure(enabled: false, home: home, environment: env)
        XCTAssertEqual(OpenCodeSessionObserver.fileState(home: home, environment: env), .missing)
        XCTAssertTrue(FileManager.default.fileExists(atPath: other.path))
        try write(Data("export const Mine = async () => ({})".utf8), to: plugin)
        XCTAssertEqual(OpenCodeSessionObserver.fileState(home: home, environment: env), .foreign)
        XCTAssertThrowsError(try OpenCodeSessionObserver.configure(enabled: true, home: home, environment: env))
        XCTAssertThrowsError(try OpenCodeSessionObserver.configure(enabled: false, home: home, environment: env))
        XCTAssertEqual(try String(contentsOf: plugin, encoding: .utf8), "export const Mine = async () => ({})")
    }

    func testObserversGoInOnceTheirClientsHaveRun() throws {
        let home = try temporaryHome(), paths = OpenAgentPaths(home: home, environment: [:])
        XCTAssertEqual(SessionObservers.observedClients(home: home), [])
        SessionObservers.installObservers(home: home)
        XCTAssertFalse(FileManager.default.fileExists(atPath: paths.openCodeConfig.path), "no OpenCode, no plugin directory")
        try FileManager.default.createDirectory(at: paths.openCode, withIntermediateDirectories: true)
        XCTAssertEqual(SessionObservers.observedClients(home: home), ["OpenCode"])
        try FileManager.default.createDirectory(at: paths.pi, withIntermediateDirectories: true)
        XCTAssertEqual(SessionObservers.observedClients(home: home), ["OpenCode", "Pi"])
        SessionObservers.installObservers(home: home)
        XCTAssertTrue(OpenCodeSessionObserver.isInstalled(home: home, environment: [:]))
        XCTAssertTrue(PiSessionObserver.isInstalled(home: home, environment: [:]))
        SessionObservers.configure(executable: URL(fileURLWithPath: "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD"),
                                   enabled: false, home: home)
        XCTAssertEqual(OpenCodeSessionObserver.fileState(home: home, environment: [:]), .missing)
        XCTAssertEqual(PiSessionObserver.fileState(home: home, environment: [:]), .missing)
    }

    func testACompletionJoinsItsSessionWithoutMakingItReportTurns() async throws {
        let home = try temporaryHome(), paths = OpenAgentPaths(home: home, environment: [:])
        let reply = #"{"id":"msg_2","sessionID":"ses_1","role":"assistant","modelID":"kimi-k2","providerID":"moonshotai","time":{"created":\#(milliseconds(now) - 20000),"completed":\#(milliseconds(now) - 10000)},"tokens":{"input":10,"output":5,"cache":{"read":20,"write":2}},"path":{"root":"/work/app"}}"#
        try write(Data(reply.utf8), to: paths.openCode.appendingPathComponent("storage/message/ses_1/msg_2.json"))
        let completion = #"{"version":1,"sessionID":"opencode:ses_1","workspace":"/work/app","title":"Fix the parser","model":"kimi-k2","providerID":"moonshotai","turnID":"msg_1","startedAtMs":\#(milliseconds(now) - 30000),"completedAtMs":\#(milliseconds(now) - 9000),"navigationTarget":{"kind":"iTermSession","id":"w0t0p0:exact-terminal"}}"#
        try write(Data(completion.utf8), to: paths.openCodeTurns.appendingPathComponent(String(repeating: "a", count: 64) + ".json"))
        let local = await OpenAgentLocalStore(paths: paths).index(since: now.addingTimeInterval(-86400))
        XCTAssertEqual(local.sessions.count, 1)
        let session = try XCTUnwrap(local.sessions.first)
        XCTAssertEqual(session.id, "opencode:ses_1")
        XCTAssertEqual(session.title, "Fix the parser")
        XCTAssertTrue(session.turns.isEmpty, "the plugin reports completions, never a running turn")
        let now = now
        let provider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in local },
            fetchQuota: { _, _ in throw ProviderFailure.format }, history: QuotaHistoryStore(), clock: { now }, ledger: .inMemory())
        let report = try await provider.fetchUsage(agents: [], historyHours: 168)
        let reported = try XCTUnwrap(report.completions.first)
        XCTAssertEqual(report.completions.count, 1)
        XCTAssertEqual(reported.vendor, "OpenCode")
        XCTAssertEqual(reported.sessionID, "opencode:ses_1")
        XCTAssertEqual(reported.task, "Fix the parser")
        XCTAssertEqual(reported.model, "kimi-k2")
        XCTAssertEqual(reported.id, SessionCompletion(sessionID: "opencode:ses_1", vendor: "OpenCode", turnID: "msg_1", task: "",
                                                      model: "", startedAt: nil, completedAt: now).id)
        XCTAssertTrue(report.turns.isEmpty)
        XCTAssertEqual(report.sessions.first?.tokensIn, 12)
        XCTAssertEqual(reported.navigationTarget, .iTermSession(id: "w0t0p0:exact-terminal"))
        XCTAssertEqual(report.sessions.first?.navigationTarget, reported.navigationTarget)
        var legacy = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(completion.utf8)) as? [String: Any])
        legacy.removeValue(forKey: "navigationTarget")
        XCTAssertNil(try OpenCodeSessionObserver.read(JSONSerialization.data(withJSONObject: legacy)).navigationTarget)

        let store = OpenAgentLocalStore(paths: paths)
        for (index, target) in [SessionNavigationTarget.iTermSession(id: "w1t2p0:new-terminal"), nil].enumerated() {
            var resumed = legacy
            resumed["turnID"] = "resumed_\(index)"
            resumed["completedAtMs"] = milliseconds(now) - Int64(1 - index) * 1000
            if let target {
                resumed["navigationTarget"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(target))
            }
            try write(JSONSerialization.data(withJSONObject: resumed),
                      to: paths.openCodeTurns.appendingPathComponent("resumed_\(index).json"))
            let merged = await store.index(since: now.addingTimeInterval(-86400))
            let refreshedProvider = OpenAgentUsageProvider(credentials: { [] }, sessions: { _ in merged },
                fetchQuota: { _, _ in throw ProviderFailure.format }, history: QuotaHistoryStore(), clock: { now }, ledger: .inMemory())
            let refreshed = try await refreshedProvider.fetchUsage(agents: [], historyHours: 168)
            XCTAssertEqual(refreshed.completions.count, index + 2)
            XCTAssertEqual(refreshed.sessions.first?.navigationTarget, target)
            XCTAssertTrue(refreshed.completions.allSatisfy { $0.navigationTarget == target },
                          "the latest source replaces or clears destinations of every earlier completion")
        }

        // A title OpenCode never replaced names nothing: the workspace folder stands in.
        let placeholder = try OpenCodeSessionObserver.read(Data(completion.replacingOccurrences(of: "Fix the parser",
            with: "New session - 2026-09-28T11:47:01.902Z").utf8))
        XCTAssertEqual(placeholder.session.title, "app")
        XCTAssertEqual(placeholder.session.completions.first?.task, "app")
        for broken in [#"{}"#, completion.replacingOccurrences(of: "opencode:ses_1", with: "pi:ses_1"),
                       completion.replacingOccurrences(of: #""turnID":"msg_1""#, with: #""turnID":"""#)] {
            XCTAssertThrowsError(try OpenCodeSessionObserver.read(Data(broken.utf8)))
        }
    }

    /// The event orders are those OpenCode 1.15.12 and 1.18.33 publish.
    func testNativePluginRecordsOnlyFinishedTurnsOfTopLevelSessions() async throws {
        let home = try temporaryHome()
        let script = home.appendingPathComponent("observer.mjs"), test = home.appendingPathComponent("test.mjs")
        try write(Data(OpenCodeSessionObserver.script.utf8), to: script)
        try write(Data(#"""
        import assert from 'node:assert/strict';
        import { mkdirSync, readdirSync, readFileSync, writeFileSync, utimesSync, existsSync } from 'node:fs';
        import { join } from 'node:path';
        import { AgentHUDSessionObserver } from './observer.mjs';
        process.env.XDG_DATA_HOME = process.argv[2];
        delete process.env.TMUX;
        process.env.TERM_PROGRAM = 'Apple_Terminal';
        process.env.ITERM_SESSION_ID = 'inherited-terminal';
        let clock = 1789000000000;
        Date.now = () => (clock += 1000);
        const directory = join(process.argv[2], 'opencode', 'agent-hud', 'turns');
        const stale = join(directory, 'a'.repeat(64) + '.json');
        mkdirSync(directory, { recursive: true });
        writeFileSync(stale, '{}');
        utimesSync(stale, new Date(clock - 8 * 86400000), new Date(clock - 8 * 86400000));
        const rows = () => readdirSync(directory).filter(f => f.endsWith('.json')).map(f => JSON.parse(readFileSync(join(directory, f), 'utf8')));
        const hooks = await AgentHUDSessionObserver({ directory: '/work/fallback' });
        assert.equal(existsSync(stale), false, 'records older than a week are pruned');
        const emit = (type, properties) => hooks.event({ event: { type, properties } });
        const session = (id, info = {}) => emit('session.updated', { sessionID: id, info: { id, title: 'Fix the parser', directory: '/work/app', ...info } });
        const user = (sessionID, id, created) => emit('message.updated', { sessionID, info: { id, sessionID, role: 'user', time: { created } } });
        const reply = (sessionID, id, parentID, created, extra = {}) => emit('message.updated', { sessionID, info: {
          id, sessionID, role: 'assistant', parentID, modelID: 'kimi-k2', providerID: 'moonshotai',
          time: extra.completed ? { created, completed: extra.completed } : { created }, finish: extra.finish, error: extra.error, summary: extra.summary } });
        const status = (sessionID, type) => emit('session.status', { sessionID, status: { type } });
        const idle = async (sessionID) => { await status(sessionID, 'idle'); await emit('session.idle', { sessionID }); };

        // A tool call, then the answer: one completion, when the session goes idle, however many idle signals follow.
        await session('ses_a');
        await user('ses_a', 'msg_01', 1);
        await status('ses_a', 'busy');
        await reply('ses_a', 'msg_02', 'msg_01', 2, { finish: 'tool-calls', completed: 3 });
        await emit('message.part.updated', { part: { type: 'text', text: 'private prompt' } });
        await reply('ses_a', 'msg_03', 'msg_01', 4, { finish: 'stop', completed: 5 });
        assert.equal(rows().length, 0);
        await idle('ses_a');
        await idle('ses_a');
        assert.equal(rows().length, 1);
        const [first] = rows();
        assert.deepEqual({ ...first, startedAtMs: 0, completedAtMs: 0 }, { version: 1, sessionID: 'opencode:ses_a', workspace: '/work/app',
          title: 'Fix the parser', model: 'kimi-k2', providerID: 'moonshotai', turnID: 'msg_01', startedAtMs: 0, completedAtMs: 0 });
        assert.ok(first.completedAtMs > first.startedAtMs);

        // An error or an abort makes the session idle before the reply that carries it closes, and once more after it.
        for (const [prompt, error] of [['msg_04', 'APIError'], ['msg_06', 'MessageAbortedError']]) {
          await user('ses_a', prompt, 6);
          await status('ses_a', 'busy');
          await reply('ses_a', prompt + 'r', prompt, 7);
          await emit('session.error', { sessionID: 'ses_a', error: { name: error } });
          await idle('ses_a');
          await reply('ses_a', prompt + 'r', prompt, 7, { error: { name: error }, completed: 8 });
          await idle('ses_a');
        }
        // A reply cut short or still calling tools is no answer.
        for (const [prompt, finish] of [['msg_08', 'length'], ['msg_10', 'tool-calls']]) {
          await user('ses_a', prompt, 9);
          await status('ses_a', 'busy');
          await reply('ses_a', prompt + 'r', prompt, 10, { finish, completed: 11 });
          await idle('ses_a');
        }
        assert.equal(rows().length, 1);

        // A context overflow is an error OpenCode recovers from: it compacts and carries on to the answer.
        await user('ses_a', 'msg_12', 12);
        await status('ses_a', 'busy');
        await reply('ses_a', 'msg_13', 'msg_12', 13);
        await emit('session.error', { sessionID: 'ses_a', error: { name: 'ContextOverflowError' } });
        await reply('ses_a', 'msg_14', 'msg_12', 14, { finish: 'stop', completed: 15, summary: true });
        await user('ses_a', 'msg_15', 15);
        await reply('ses_a', 'msg_16', 'msg_15', 16, { finish: 'stop', completed: 17 });
        await idle('ses_a');
        assert.deepEqual(rows().map(r => r.turnID).sort(), ['msg_01', 'msg_15']);
        // A summary that /compact wrote answers nothing.
        await user('ses_a', 'msg_18', 18);
        await status('ses_a', 'busy');
        await reply('ses_a', 'msg_19', 'msg_18', 19, { finish: 'stop', completed: 20, summary: true });
        await idle('ses_a');
        assert.equal(rows().length, 2);
        // An idle that arrives before the answer closes waits for it.
        await user('ses_a', 'msg_21', 21);
        await status('ses_a', 'busy');
        await reply('ses_a', 'msg_22', 'msg_21', 22, { finish: 'stop' });
        await idle('ses_a');
        assert.equal(rows().length, 2);
        await reply('ses_a', 'msg_22', 'msg_21', 22, { finish: 'stop', completed: 23 });
        assert.equal(rows().length, 3);

        // A sub-agent's session finishes inside its parent's turn.
        await emit('session.created', { sessionID: 'ses_child', info: { id: 'ses_child', parentID: 'ses_a', title: 'Look around (@general subagent)' } });
        await user('ses_child', 'msg_c1', 20);
        await status('ses_child', 'busy');
        await reply('ses_child', 'msg_c2', 'msg_c1', 21, { finish: 'stop', completed: 22 });
        await idle('ses_child');
        assert.equal(rows().length, 3);

        // A prompt queued during a turn is answered in the same busy stretch, while OpenCode updates the first prompt again.
        await session('ses_b', { directory: undefined, title: 'New session - 2026-09-28T11:47:01.902Z' });
        await user('ses_b', 'msg_30', 30);
        await status('ses_b', 'busy');
        await reply('ses_b', 'msg_31', 'msg_30', 31, { finish: 'stop', completed: 32 });
        await user('ses_b', 'msg_33', 33);
        await user('ses_b', 'msg_30', 30);
        await reply('ses_b', 'msg_31', 'msg_30', 31, { finish: 'stop', completed: 32 });
        await reply('ses_b', 'msg_34', 'msg_33', 34, { finish: 'stop', completed: 35 });
        await idle('ses_b');
        const queued = rows().filter(r => r.sessionID === 'opencode:ses_b');
        assert.deepEqual(queued.map(r => [r.turnID, r.workspace]), [['msg_33', '/work/fallback']]);
        assert.equal(JSON.stringify(rows()).includes('private prompt'), false);
        // A session deleted in OpenCode takes its records along; the others stay.
        const kept = rows().length - queued.length;
        await emit('session.deleted', { info: { id: 'ses_b' } });
        assert.equal(rows().filter(r => r.sessionID === 'opencode:ses_b').length, 0);
        assert.equal(rows().length, kept);

        for (const [index, [program, id, tmux, expected]] of [
          ['iTerm.app', 'w7t2p0:exact-terminal', '', { kind: 'iTermSession', id: 'w7t2p0:exact-terminal' }],
          ['iTerm.app', 'w7t2p0:exact-terminal', '/tmp/tmux,123,0', undefined],
          ['Apple_Terminal', 'w7t2p0:exact-terminal', '', undefined],
          ['iTerm.app', '', '', undefined],
        ].entries()) {
          process.env.TERM_PROGRAM = program;
          process.env.ITERM_SESSION_ID = id;
          process.env.TMUX = tmux;
          const sid = 'navigation_' + index;
          await session(sid);
          await user(sid, 'prompt_' + index, 40 + index * 2);
          await status(sid, 'busy');
          await reply(sid, 'reply_' + index, 'prompt_' + index, 41 + index * 2, { finish: 'stop', completed: 50 + index });
          await idle(sid);
          assert.deepEqual(rows().find(r => r.sessionID === 'opencode:' + sid)?.navigationTarget, expected);
        }

        // An unwritable inbox cannot break a turn.
        process.env.XDG_DATA_HOME = join(process.argv[2], 'blocked');
        writeFileSync(process.env.XDG_DATA_HOME, 'file instead of directory');
        const blocked = await AgentHUDSessionObserver({ directory: '/work/app' });
        await blocked.event({ event: { type: 'session.updated', properties: { info: { id: 'ses_c' } } } });
        await blocked.event({ event: { type: 'message.updated', properties: { info: { id: 'msg_40', sessionID: 'ses_c', role: 'assistant', parentID: 'msg_39', finish: 'stop', time: { created: 1, completed: 2 } } } } });
        await blocked.event({ event: { type: 'session.idle', properties: { sessionID: 'ses_c' } } });
        await blocked.event({ event: { type: 'session.idle' } });
        await blocked.event({});
        await blocked.event(undefined);
        console.log('OpenCode plugin checks passed');
        """#.utf8), to: test)
        let output = try await ProviderCommand.run("/usr/bin/env", ["node", test.path, home.path])
        XCTAssertTrue(output.contains("OpenCode plugin checks passed"), output)
    }
}
