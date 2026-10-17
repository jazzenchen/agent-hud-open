import AgentHUDSupport
import Foundation
import XCTest
@testable import AgentHUDCore

/// The session rows and the token chart under a layout's filters, over the sample: two sessions running (Claude's and
/// Codex's), six ended in the last day (three Claude, two Codex, one DeepSeek) and one Codex session ended before that,
/// or over sessions of their own where the order or the projects need them; and what the tools read and take of them.
final class HUDToolsFiltersTests: HUDToolsCase {
    private func rows(_ sessions: PanelLayout.Sessions = PanelLayout.Sessions(), vendor: String? = nil) -> PanelSessionRows {
        store.panelSessions(PanelSessionFilter(sessions), vendor: vendor)
    }

    private func ids(_ rows: PanelSessionRows) -> [String] { rows.shown.map(\.id) }

    // MARK: Rows

    func testTheDefaultRowsAreFiveOfTheDaysSessionsWithWorkInFlightFirst() {
        let rows = rows()
        XCTAssertEqual(ids(rows), ["s1", "s2", "s4", "s7", "s8"])
        XCTAssertEqual(rows.running.map(\.id), ["s1", "s2"])
        XCTAssertEqual(rows.more, 3, "the day's other ended sessions; the one before it is not counted")
        XCTAssertEqual(rows.counts, [.running: 2, .ended: 6])
        XCTAssertFalse(rows.noneMatch)
    }

    func testTheLimitSetsTheRowsAndMoreCountsTheRest() {
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(limit: 3))), ["s1", "s2", "s4"])
        XCTAssertEqual(rows(PanelLayout.Sessions(limit: 3)).more, 5)
        let ten = rows(PanelLayout.Sessions(limit: 10))
        XCTAssertEqual(ids(ten), ["s1", "s2", "s4", "s7", "s8", "s3", "s5", "s6"])
        XCTAssertEqual(ten.more, 0)
    }

    func testStatesAndAgentsChooseTheRowsInTheSameOrderAndMoreCountsWhatTheyKeep() {
        let running = rows(PanelLayout.Sessions(states: [.running]))
        XCTAssertEqual(ids(running), ["s1", "s2"])
        XCTAssertEqual(running.more, 0)
        let ended = rows(PanelLayout.Sessions(states: [.ended]))
        XCTAssertEqual(ids(ended), ["s4", "s7", "s8", "s3", "s5"])
        XCTAssertEqual(ended.more, 1)
        XCTAssertTrue(ended.running.isEmpty, "the header counts only what the filter keeps")
        XCTAssertEqual(ended.counts, [.running: 2, .ended: 6], "the counts are before the filter")

        let codex = rows(PanelLayout.Sessions(agents: ["Codex"]))
        XCTAssertEqual(ids(codex), ["s2", "s4", "s7"])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(agents: ["ChatGPT"]))), ids(codex), "matched by mark")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(limit: 2, agents: ["Claude", "DeepSeek"], states: [.ended]))), ["s8", "s3"])
        XCTAssertEqual(rows(PanelLayout.Sessions(limit: 2, agents: ["Claude", "DeepSeek"], states: [.ended])).more, 2)
    }

    func testAFilterThatKeepsNothingSaysSoAndNoSessionsAtAllDoesNot() {
        let none = rows(PanelLayout.Sessions(agents: ["Grok CLI"]))
        XCTAssertTrue(none.shown.isEmpty)
        XCTAssertEqual(none.more, 0)
        XCTAssertTrue(none.noneMatch)
        XCTAssertTrue(rows(PanelLayout.Sessions(states: [.waiting])).noneMatch)

        // With no session in the day at all, the section has none yet, whatever the filter.
        store.replace(report: UsageReport(generatedAt: DemoClock.now, snapshots: [], sessions: [], usage: []))
        let empty = rows(PanelLayout.Sessions(agents: ["Grok CLI"]))
        XCTAssertTrue(empty.shown.isEmpty)
        XCTAssertFalse(empty.noneMatch)
    }

    /// An agent's own panel lists its agent's sessions, and the layout's filter applies among them.
    func testAnAgentsOwnPanelAppliesItsAgentAndTheFilter() {
        XCTAssertEqual(ids(rows(vendor: "Claude")), ["s1", "s3", "s5", "s6"])
        XCTAssertEqual(rows(vendor: "Claude").counts, [.running: 1, .ended: 3])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(limit: 2, states: [.ended]), vendor: "Claude")), ["s3", "s5"])
        let other = rows(PanelLayout.Sessions(agents: ["Codex"]), vendor: "Claude")
        XCTAssertTrue(other.shown.isEmpty)
        XCTAssertTrue(other.noneMatch, "Claude's panel has sessions; the layout keeps none of them")
    }

    // MARK: The token chart's columns

    func testEachPeriodIsDrawnInItsColumnsEndingAtTheSameQuarterAndKeptForTheReport() throws {
        let day = store.hudTokenColumns(consumerIDs: nil), week = store.hudTokenColumns(consumerIDs: nil, period: .week)
        let month = store.hudTokenColumns(consumerIDs: nil, period: .month)
        XCTAssertEqual(day.count, 96)
        XCTAssertTrue((7...8).contains(week.count), "\(week.count) days")
        XCTAssertTrue((30...31).contains(month.count), "\(month.count) days")
        XCTAssertTrue(week.allSatisfy { $0.interval.duration >= 23 * 3600 })
        for period in PanelTokenPeriod.allCases {
            let interval = store.hudTokenInterval(period)
            XCTAssertEqual(interval.end, store.hudTokenInterval.end)
            XCTAssertEqual(interval.duration, period.duration)
        }
        func total(_ columns: [TokenColumn]) -> Int { columns.reduce(0) { $0 + $1.total } }
        XCTAssertLessThan(total(day), total(week))
        XCTAssertLessThan(total(week), total(month), "the sample has a month of tokens")

        let codex = Set(store.consumers.filter { $0.vendor == "Codex" }.map(\.id))
        XCTAssertLessThan(total(store.hudTokenColumns(consumerIDs: codex, period: .week)), total(week))
        XCTAssertEqual(store.panelTokenConsumers(PanelTokenFilter(PanelLayout.Tokens(agents: ["ChatGPT"]))), codex)
        XCTAssertNil(store.panelTokenConsumers(PanelTokenFilter()), "nothing narrows the default chart")
        XCTAssertEqual(store.panelTokenConsumers(PanelTokenFilter(PanelLayout.Tokens(agents: ["Codex"])), vendor: "Claude"), [],
                       "an agent's own panel and the layout both apply")

        // Kept for the report: the same report gives the same columns, and a new one is counted again.
        XCTAssertEqual(store.hudTokenColumns(consumerIDs: nil, period: .week), week)
        let consumers = try XCTUnwrap(store.report).consumers
        store.replace(report: UsageReport(generatedAt: DemoClock.now, snapshots: [], sessions: [], consumers: consumers, usage: []))
        XCTAssertEqual(total(store.hudTokenColumns(consumerIDs: nil, period: .week)), 0)
    }

    // MARK: Reading

    func testThePanelIsReadWithItsSessionsAndTokens() throws {
        let panel = try result(MCPTools.getHUD)
        // The sample's folders are in the home folder, written with ~; which of them are one project depends on the Git
        // repositories this Mac keeps there, so the projects are those the sessions resolve to here.
        let projects = SessionProject.ordered(Set(store.view.sessions.map { SessionProject($0.session) }))
        XCTAssertFalse(projects.isEmpty)
        XCTAssertEqual(panel["sessions"], .object([
            "limit": .integer(5), "period": .string("day"), "sort": .string("active"), "agents": .array([]),
            "projects": .array([]), "states": .array(["waiting", "running", "ended"].map(JSONValue.string)),
            "offeredAgents": .array(["Claude", "Codex", "DeepSeek"].map(JSONValue.string)),
            "offeredProjects": .array(projects.map { .object(["id": .string($0.id), "name": .string($0.title)]) }),
            "shown": .integer(5), "more": .integer(3),
            "byState": .object(["waiting": .integer(0), "running": .integer(2), "ended": .integer(6)]),
        ]))
        let day = store.hudTokenColumns(consumerIDs: nil).reduce(0) { $0 + $1.total }
        let models = store.consumers.map { model -> JSONValue in
            .object(["id": .string(model.id), "name": .string(model.name), "agent": .string(model.displayVendor)])
        }
        XCTAssertEqual(panel["tokens"], .object([
            "period": .string("day"), "agents": .array([]), "models": .array([]),
            "offeredAgents": .array(["Claude", "Codex", "DeepSeek"].map(JSONValue.string)), "offeredModels": .array(models),
            "total": .integer(Int64(day)),
        ]))
        XCTAssertGreaterThan(models.count, 3, "the sample's agents spend tokens on several models")
        XCTAssertTrue(models.contains(.object(["id": .string("claude-model:claude-opus-5-5"), "name": .string("Opus 5.5"),
                                               "agent": .string("Claude")])), "an id, the name the panel writes, and its agent")
    }

    /// Counts, agents' names and projects only: a session's title and id never reach an agent, and its folder only as
    /// its project's, written with ~.
    func testWhatIsReadOfSessionsNamesNoTitleOrIdAndFoldersOnlyAsProjects() throws {
        let text = String(decoding: try RecordCoding.encoder().encode(JSONValue.object(try result(MCPTools.getHUD))), as: UTF8.self)
        for session in store.view.sessions.map(\.session) {
            XCTAssertFalse(text.contains(session.task), session.task)
            XCTAssertFalse(text.contains("\"\(session.id)\""), session.id)
            let folder = try XCTUnwrap(session.workingDirectory)
            XCTAssertFalse(text.contains(folder), "\(folder) is written with ~")
            XCTAssertTrue(text.contains("\"\(SessionProject(session).id)\""), folder)
        }
    }

    // MARK: Proposing

    func testSessionsAndTokensAreProposedAndToldBack() throws {
        let panel = try result(MCPTools.draftHUD, [
            "sessions": .object(["limit": .integer(3), "agents": .array([.string("ChatGPT")]),
                                 "states": .array([.string("ended"), .string("running")])]),
            "tokens": .object(["period": .string("week"), "agents": .array([.string("Claude")])]),
        ])
        XCTAssertEqual(layout.draft, PanelLayout(sessions: PanelLayout.Sessions(limit: 3, agents: ["ChatGPT"], states: [.running, .ended]),
                                                 tokens: PanelLayout.Tokens(period: .week, agents: ["Claude"])))
        let sessions = try XCTUnwrap(panel["sessions"]?.objectValue)
        XCTAssertEqual(sessions["limit"], .integer(3))
        XCTAssertEqual(sessions["agents"], .array([.string("ChatGPT")]))
        XCTAssertEqual(sessions["states"], .array([.string("running"), .string("ended")]))
        XCTAssertEqual(sessions["shown"], .integer(3), "s2, s4, s7")
        XCTAssertEqual(sessions["more"], .integer(0))
        let tokens = try XCTUnwrap(panel["tokens"]?.objectValue)
        XCTAssertEqual(tokens["period"], .string("week"))
        XCTAssertEqual(tokens["agents"], .array([.string("Claude")]))
        let claude = Set(store.consumers.filter { $0.vendor == "Claude" }.map(\.id))
        XCTAssertEqual(tokens["total"], .integer(Int64(store.hudTokenColumns(consumerIDs: claude, period: .week).reduce(0) { $0 + $1.total })))
        XCTAssertEqual(changes, 0, "a proposal writes no setting")

        // The default spelled out is the default layout: no draft.
        layout.discard()
        let spelled = try result(MCPTools.draftHUD, [
            "sessions": .object(["limit": .integer(5), "states": .array(["waiting", "running", "ended"].map(JSONValue.string))]),
            "tokens": .object(["period": .string("day"), "agents": .array([])]),
        ])
        XCTAssertFalse(layout.hasDraft)
        XCTAssertEqual(spelled["unsaved"], .bool(false))
    }

    func testSessionsAndTokensAreReadStrictly() throws {
        let typed = try refusal([
            "sessions": .object(["limit": .integer(11), "states": .array([.string("idle"), .integer(2)]),
                                 "agents": .string("Claude"), "project": .string("hud")]),
            "tokens": .object(["period": .string("year"), "agents": .array([.string("")]), "model": .string("opus")]),
        ])
        for part in ["sessions.limit is a whole number from 1 to 10.",
                     "sessions.states[0]: \"idle\" is not one of \"waiting\", \"running\", \"ended\".",
                     "sessions.states[1] is a string.", "sessions.agents is a list.",
                     "sessions has no property \"project\"; sessions take limit, period, sort, agents, projects and states.",
                     "tokens.period is one of \"day\", \"week\", \"month\".",
                     "tokens.agents[0] is an agent's name from get_hud's tokens.offeredAgents.",
                     "tokens has no property \"model\"; tokens take period, agents and models."] {
            XCTAssertTrue(typed.contains(part), "\(part) — \(typed)")
        }
        for limit in [JSONValue.integer(0), .number(2.5), .string("3"), .integer(-1)] {
            XCTAssertTrue(try refusal(["sessions": .object(["limit": limit])]).contains("sessions.limit is a whole number from 1 to 10."),
                          "\(limit)")
        }
        let named = try refusal(["sessions": .object(["sort": .string("title"), "projects": .array([.integer(1), .string("")])])])
        for part in ["sessions.sort is one of \"active\", \"date\".", "sessions.projects[0] is a string.",
                     "sessions.projects[1] is a project's id from get_hud's sessions.offeredProjects, or a path inside one."] {
            XCTAssertTrue(named.contains(part), "\(part) — \(named)")
        }
        XCTAssertTrue(try refusal(["sessions": .object(["projects": .string("~/work/agent-hud")])])
            .contains("sessions.projects is a list."))
        XCTAssertTrue(try refusal(["sessions": .integer(3)]).contains("sessions is an object."))
        XCTAssertTrue(try refusal(["tokens": .string("week")]).contains("tokens is an object."))
    }

    func testAnAgentWithNoSessionsOrTokensIsRefusedUnlessTheLayoutHoldsIt() throws {
        let sentence = try refusal(["sessions": .object(["agents": .array([.string("Grok CLI")])]),
                                    "tokens": .object(["agents": .array([.string("Gemini")])])])
        XCTAssertTrue(sentence.contains("sessions.agents[0]: \"Grok CLI\" has no session now. Agents with sessions now: "
                                        + "\"Claude\", \"Codex\", \"DeepSeek\"."), sentence)
        XCTAssertTrue(sentence.contains("tokens.agents[0]: \"Gemini\" has no model in the token chart now. Agents in the chart "
                                        + "now: \"Claude\", \"Codex\", \"DeepSeek\"."), sentence)

        // Saved by a person while it had sessions; the layout that was read can be sent back with it.
        layout.propose(PanelLayout(sessions: PanelLayout.Sessions(agents: ["Grok CLI", "Claude"])))
        layout.save()
        let read = try XCTUnwrap(try result(MCPTools.getHUD)["layout"])
        var arguments = try XCTUnwrap(read.objectValue)
        arguments["tokens"] = .object(["period": .string("month")])
        _ = try result(MCPTools.draftHUD, arguments)
        XCTAssertEqual(layout.draft?.sessions.agents, ["Claude", "Grok CLI"])
        XCTAssertEqual(layout.draft?.tokens.period, .month)
    }

    // MARK: Periods

    func testThePeriodSetsHowFarBackEndedSessionsAreListed() {
        let day = rows(PanelLayout.Sessions(limit: 10))
        XCTAssertEqual(ids(day), ["s1", "s2", "s4", "s7", "s8", "s3", "s5", "s6"], "those running, and those ended in the day")
        let week = rows(PanelLayout.Sessions(limit: 10, period: .week))
        XCTAssertEqual(ids(week), ["s1", "s2", "s4", "s7", "s8", "s3", "s5", "s6", "s9"])
        XCTAssertEqual(week.counts, [.running: 2, .ended: 7], "the counts are the period's")
        XCTAssertEqual(rows(PanelLayout.Sessions(period: .week)).more, 4, "five rows of nine")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(period: .week, agents: ["Codex"], states: [.ended]))), ["s4", "s7", "s9"])
    }

    /// A week whose sessions the filter leaves out says none match; a day that has no session says there are none, as
    /// the default view does.
    func testAWeekWhoseSessionsTheFilterLeavesOutSaysNoneMatch() throws {
        let sample = try XCTUnwrap(store.report)
        store.replace(report: UsageReport(generatedAt: sample.generatedAt, snapshots: sample.snapshots,
                                          sessions: sample.sessions.filter { $0.id == "s9" },
                                          consumers: sample.consumers, usage: sample.usage))
        XCTAssertTrue(rows().shown.isEmpty)
        XCTAssertFalse(rows().noneMatch, "the day has no session")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(period: .week))), ["s9"])
        XCTAssertTrue(rows(PanelLayout.Sessions(period: .week, agents: ["Claude"])).noneMatch, "the week's session is Codex's")
    }

    func testThePeriodIsProposedAndToldBack() throws {
        let panel = try result(MCPTools.draftHUD, ["sessions": .object(["period": .string("week"), "limit": .integer(10)])])
        XCTAssertEqual(layout.draft, PanelLayout(sessions: PanelLayout.Sessions(limit: 10, period: .week)))
        let sessions = try XCTUnwrap(panel["sessions"]?.objectValue)
        XCTAssertEqual(sessions["period"], .string("week"))
        XCTAssertEqual(sessions["shown"], .integer(9))
        XCTAssertEqual(sessions["byState"], .object(["waiting": .integer(0), "running": .integer(2), "ended": .integer(7)]))
        XCTAssertEqual(try result(MCPTools.getHUD)["layout"],
                       .object(["sessions": .object(["limit": .integer(10), "period": .string("week")])]))
        // The build before's names are read from a stored layout, but a proposal of one is told the names there are.
        for period in ["7d", "3d", "24h", "1h", "30d", "month"] {
            let sentence = try refusal(["sessions": .object(["period": .string(period)])])
            XCTAssertTrue(sentence.contains(#"sessions.period is one of "day", "week"."#), sentence)
        }

        layout.discard()
        _ = try result(MCPTools.draftHUD, ["sessions": .object(["period": .string("day")])])
        XCTAssertFalse(layout.hasDraft, "a day is the default view's")
    }

    // MARK: Order and projects

    /// Five sessions of the sample's agents, in folders of their own: two running, one of them quiet for 20 minutes,
    /// two ended in the day and one two days ago.
    private func keepOwnSessions() throws {
        let sample = try XCTUnwrap(store.report), now = DemoClock.now
        func session(_ id: String, _ agent: String, minutesAgo: Double, running: Bool = false, in folder: String?) -> LiveSession {
            let last = now.addingTimeInterval(-minutesAgo * 60)
            return LiveSession(id: id, agentId: agent, task: "Task \(id)", terminal: nil, startedAt: last.addingTimeInterval(-600),
                               endedAt: running ? nil : last, pctOfWindow: nil, tokensIn: 1, tokensOut: 1,
                               observedAt: running ? now.addingTimeInterval(-60) : last, workingDirectory: folder,
                               lastActivityAt: last)
        }
        let claude = DemoData.consumerID("Claude", "claude-opus-5-5"), codex = DemoData.consumerID("Codex", "gpt-6.1-sol")
        store.replace(report: UsageReport(generatedAt: now, snapshots: sample.snapshots, sessions: [
            session("r1", claude, minutesAgo: 20, running: true, in: "/agent-hud-tests/alpha"),
            session("r2", codex, minutesAgo: 1, running: true, in: "/agent-hud-tests/beta"),
            session("e1", claude, minutesAgo: 5, in: "/agent-hud-tests/alpha"),
            session("e2", codex, minutesAgo: 40, in: nil),
            session("old", codex, minutesAgo: 2 * 24 * 60, in: "/agent-hud-tests/beta"),
        ], consumers: sample.consumers, usage: sample.usage))
    }

    func testTheDateOrderListsEverySessionByItsLastEventWhateverItsState() throws {
        try keepOwnSessions()
        let active = rows()
        XCTAssertEqual(ids(active), ["r2", "r1", "e1", "e2"], "in flight first, the newest started first")
        let date = rows(PanelLayout.Sessions(sort: .date))
        XCTAssertEqual(ids(date), ["r2", "e1", "r1", "e2"], "by the last event, whatever the state")
        XCTAssertEqual(date.counts, active.counts)
        XCTAssertEqual(date.counts, [.running: 2, .ended: 2])

        let two = rows(PanelLayout.Sessions(limit: 2, sort: .date))
        XCTAssertEqual(ids(two), ["r2", "e1"])
        XCTAssertEqual(two.more, 2, "+2 more counts r1 and e2")
        XCTAssertEqual(two.running.map(\.id), ["r2", "r1"], "the header counts every running session the filter keeps")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(limit: 2))), ["r2", "r1"])
        XCTAssertEqual(rows(PanelLayout.Sessions(limit: 2)).more, 2)

        XCTAssertEqual(ids(rows(PanelLayout.Sessions(sort: .date, states: [.ended]))), ["e1", "e2"])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(limit: 10, period: .week, sort: .date))), ["r2", "e1", "r1", "e2", "old"])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(sort: .date), vendor: "Claude")), ["e1", "r1"])
    }

    func testProjectsChooseTheRowsInTheSameOrderAndTheCountsAreBeforeThem() throws {
        try keepOwnSessions()
        let alpha = rows(PanelLayout.Sessions(projects: ["/agent-hud-tests/alpha"]))
        XCTAssertEqual(ids(alpha), ["r1", "e1"])
        XCTAssertEqual(alpha.running.map(\.id), ["r1"])
        XCTAssertEqual(alpha.counts, [.running: 2, .ended: 2], "the counts are before the projects")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(sort: .date, projects: ["/agent-hud-tests/alpha"]))), ["e1", "r1"])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(projects: ["unassigned"]))), ["e2"], "the sessions of no folder")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(projects: ["/agent-hud-tests/beta"]))), ["r2"])
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(period: .week, projects: ["/agent-hud-tests/beta"]))), ["r2", "old"])
        let both = rows(PanelLayout.Sessions(limit: 1, projects: ["/agent-hud-tests/beta", "unassigned"]))
        XCTAssertEqual(ids(both), ["r2"])
        XCTAssertEqual(both.more, 1)
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(agents: ["Codex"], projects: ["/agent-hud-tests/alpha"]))), [])

        let away = rows(PanelLayout.Sessions(projects: ["/agent-hud-tests/gamma"]))
        XCTAssertTrue(away.shown.isEmpty)
        XCTAssertTrue(away.noneMatch, "a project that has no session now matches nothing")
        XCTAssertEqual(ids(rows(PanelLayout.Sessions(projects: ["/agent-hud-tests/alpha"]), vendor: "Claude")), ["r1", "e1"])
        XCTAssertTrue(rows(PanelLayout.Sessions(projects: ["/agent-hud-tests/alpha"]), vendor: "Codex").noneMatch)
    }

    /// A session's project is found once for each folder a report names, and found again for the next report: a folder
    /// that becomes a Git checkout counts as one from the report after.
    func testAProjectIsFoundOnceAReport() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("HUD Project Cache \(UUID().uuidString)")
            .resolvingSymlinksInPath()
        defer { try? FileManager.default.removeItem(at: folder) }
        let inside = folder.appendingPathComponent("Sources")
        try FileManager.default.createDirectory(at: inside, withIntermediateDirectories: true)
        let session = LiveSession(id: "x", agentId: DemoData.consumerID("Claude", "claude-opus-5-5"), task: "Task",
                                  terminal: nil, startedAt: DemoClock.now, pctOfWindow: nil, tokensIn: 0, tokensOut: 0,
                                  workingDirectory: inside.path)
        let report = UsageReport(generatedAt: DemoClock.now, snapshots: [], sessions: [session], usage: [])
        store.replace(report: report)
        XCTAssertEqual(store.sessionProject(session), .directory(inside.path))
        try FileManager.default.createDirectory(at: folder.appendingPathComponent(".git"), withIntermediateDirectories: true)
        XCTAssertEqual(store.sessionProject(session), .directory(inside.path), "kept for the report")
        XCTAssertEqual(store.panelSessionProjects, [.directory(inside.path)])
        store.replace(report: report)
        XCTAssertEqual(store.sessionProject(session), .directory(folder.path), "found again for the next")
        XCTAssertEqual(store.sessionProject(LiveSession(id: "y", agentId: "a", task: "", terminal: nil, startedAt: DemoClock.now,
                                                        pctOfWindow: nil, tokensIn: 0, tokensOut: 0)), .unassigned)
    }

    func testTheOrderAndTheProjectsAreReadProposedAndToldBack() throws {
        try keepOwnSessions()
        let read = try XCTUnwrap(try result(MCPTools.getHUD)["sessions"]?.objectValue)
        XCTAssertEqual(read["offeredProjects"], .array([
            .object(["id": .string("/agent-hud-tests/alpha"), "name": .string("alpha")]),
            .object(["id": .string("/agent-hud-tests/beta"), "name": .string("beta")]),
            .object(["id": .string("unassigned"), "name": .string("Unassigned project")]),
        ]))

        // A path inside a project names it, and the layout lists it by its id.
        let panel = try result(MCPTools.draftHUD, ["sessions": .object([
            "sort": .string("date"), "projects": .array([.string("unassigned"), .string("/agent-hud-tests/alpha/Sources/App")]),
        ])])
        XCTAssertEqual(layout.draft, PanelLayout(sessions: PanelLayout.Sessions(sort: .date, projects: ["/agent-hud-tests/alpha",
                                                                                                         "unassigned"])))
        let sessions = try XCTUnwrap(panel["sessions"]?.objectValue)
        XCTAssertEqual(sessions["sort"], .string("date"))
        XCTAssertEqual(sessions["projects"], .array([.string("/agent-hud-tests/alpha"), .string("unassigned")]))
        XCTAssertEqual(sessions["shown"], .integer(3), "e1, r1 and e2")
        XCTAssertEqual(sessions["more"], .integer(0))
        let note = try XCTUnwrap(panel["note"]?.stringValue)
        XCTAssertTrue(note.contains("\"/agent-hud-tests/alpha/Sources/App\" names the project \"/agent-hud-tests/alpha\", which "
                                    + "sessions.projects lists by that id."), note)
        XCTAssertEqual(changes, 0, "a proposal writes no setting")

        // What get_hud returned, sent back, changes nothing; the default spelled out is the default layout.
        let revision = layout.revision
        let arguments = try XCTUnwrap(try result(MCPTools.getHUD)["layout"]?.objectValue)
        XCTAssertEqual(arguments["sessions"], .object(["sort": .string("date"),
                                                      "projects": .array([.string("/agent-hud-tests/alpha"), .string("unassigned")])]))
        _ = try result(MCPTools.draftHUD, arguments)
        XCTAssertEqual(layout.draft?.sessions, PanelLayout.Sessions(sort: .date, projects: ["/agent-hud-tests/alpha", "unassigned"]))
        XCTAssertEqual(layout.revision, revision, "the layout that was read, sent back, keeps the draft as it is")
        layout.discard()
        _ = try result(MCPTools.draftHUD, ["sessions": .object(["sort": .string("active"), "projects": .array([])])])
        XCTAssertFalse(layout.hasDraft)
    }

    func testAProjectWithNoSessionsIsRefusedUnlessTheLayoutHoldsIt() throws {
        try keepOwnSessions()
        let sentence = try refusal(["sessions": .object(["projects": .array([.string("/agent-hud-tests/gamma"), .string("beta")])])])
        XCTAssertTrue(sentence.contains("sessions.projects[0]: \"/agent-hud-tests/gamma\" is no project's id from get_hud's "
                                        + "sessions.offeredProjects, nor a path inside one. Projects with sessions now: "
                                        + "\"/agent-hud-tests/alpha\", \"/agent-hud-tests/beta\", \"unassigned\"."), sentence)
        XCTAssertTrue(sentence.contains("sessions.projects[1]: \"beta\" is a project's name, not its id. Name it by its id: "
                                        + "\"/agent-hud-tests/beta\"."), sentence)

        // Saved while it had sessions; the layout that was read can be sent back with it.
        layout.propose(PanelLayout(sessions: PanelLayout.Sessions(projects: ["/agent-hud-tests/gamma", "/agent-hud-tests/alpha"])))
        layout.save()
        var arguments = try XCTUnwrap(try result(MCPTools.getHUD)["layout"]?.objectValue)
        arguments["tokens"] = .object(["period": .string("month")])
        _ = try result(MCPTools.draftHUD, arguments)
        XCTAssertEqual(layout.draft?.sessions.projects, ["/agent-hud-tests/alpha", "/agent-hud-tests/gamma"])
        XCTAssertEqual(layout.draft?.tokens.period, .month)
    }

    // MARK: Models

    func testModelsAreProposedReadAndRefusedWhereTheChartCannotDrawThem() throws {
        let opus = "claude-model:claude-opus-5-5", sol = "codex-model:gpt-6.1-sol"
        let panel = try result(MCPTools.draftHUD, ["tokens": .object(["models": .array([.string(sol), .string(opus)])])])
        XCTAssertEqual(layout.draft, PanelLayout(tokens: PanelLayout.Tokens(models: [opus, sol])))
        let tokens = try XCTUnwrap(panel["tokens"]?.objectValue)
        XCTAssertEqual(tokens["models"], .array([.string(opus), .string(sol)]))
        XCTAssertEqual(tokens["total"], .integer(Int64(store.hudTokenColumns(consumerIDs: [opus, sol]).reduce(0) { $0 + $1.total })))
        XCTAssertEqual(tokens["offeredModels"]?.arrayValue?.count, store.consumers.count, "every model the chart can draw")
        XCTAssertEqual(store.panelTokenConsumers(PanelTokenFilter(try XCTUnwrap(layout.draft).tokens)), [opus, sol])
        XCTAssertEqual(store.panelTokenConsumers(PanelTokenFilter(try XCTUnwrap(layout.draft).tokens), vendor: "Claude"), [opus],
                       "an agent's own panel draws its agent's among them")

        let refused = try refusal(["tokens": .object([
            "agents": .array([.string("Claude")]),
            "models": .array([.string(opus), .string(sol), .string("gemini-model:pro"), .string(""), .integer(3)]),
        ])])
        for part in ["tokens.models[1]: \"\(sol)\" is Codex's, and tokens.agents leaves Codex out, so the chart would not draw it. "
                        + "Name \"Codex\" in tokens.agents too, or name no agent.",
                     "tokens.models[2]: \"gemini-model:pro\" is no model in the token chart now. Models in the chart now: "
                        + "\"claude-model:claude-fable-5-1\",",
                     "tokens.models[3] is a model's id from get_hud's tokens.offeredModels.",
                     "tokens.models[4] is a string."] {
            XCTAssertTrue(refused.contains(part), "\(part) — \(refused)")
        }
        XCTAssertFalse(refused.contains("tokens.models[0]"), "Opus is Claude's")
        XCTAssertTrue(try refusal(["tokens": .object(["models": .string(opus)])]).contains("tokens.models is a list."))
    }

    func testAModelTheLayoutHoldsCanBeSentBackWhileItSpendsNoTokens() throws {
        let opus = "claude-model:claude-opus-5-5"
        layout.propose(PanelLayout(tokens: PanelLayout.Tokens(models: [opus])))
        layout.save()
        let sample = try XCTUnwrap(store.report)
        store.replace(report: UsageReport(generatedAt: sample.generatedAt, snapshots: sample.snapshots, sessions: sample.sessions,
                                          consumers: sample.consumers.filter { $0.id != opus }, usage: sample.usage))
        var arguments = try XCTUnwrap(try result(MCPTools.getHUD)["layout"]?.objectValue)
        XCTAssertEqual(arguments["tokens"], .object(["models": .array([.string(opus)])]))
        arguments["tokens"] = .object(["models": .array([.string(opus)]), "period": .string("week")])
        _ = try result(MCPTools.draftHUD, arguments)
        XCTAssertEqual(layout.draft?.tokens, PanelLayout.Tokens(period: .week, models: [opus]))
        XCTAssertTrue(try refusal(["tokens": .object(["models": .array([.string("codex-model:none")])])]).contains("tokens.models[0]:"))
    }

    // MARK: A block's period

    func testABlocksPeriodIsProposedReadAndRefusedWhereItDrawsNoChart() throws {
        let panel = try result(MCPTools.draftHUD, ["blocks": .object([
            "Claude": .object(["widget": .string("chart"), "period": .string("week")]),
            "api:DeepSeek": .object(["period": .string("month")]),
        ])])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .chart, period: .week),
                                                          "api:DeepSeek": PanelLayout.Block(period: .month)]))
        XCTAssertEqual(try block("Claude", in: panel)["period"], .string("week"))
        XCTAssertEqual(try block("api:DeepSeek", in: panel)["period"], .string("month"))
        XCTAssertNil(try block("Codex", in: panel)["period"], "bars cover no period")

        let refused = try refusal(["blocks": .object([
            "Codex": .object(["widget": .string("ring"), "period": .string("day")]),
            "Claude": .object(["widget": .string("chart"), "period": .string("year")]),
        ])])
        for part in [#"blocks.Codex.period: The "ring" widget of "Codex" draws no chart. period applies to the "chart" widget, "#
                        + #"whose bars cover it, and to API balances, whose estimated cost, burn rate and chart cover it."#,
                     #"blocks.Claude.period is one of "day", "week", "month"."#] {
            XCTAssertTrue(refused.contains(part), "\(part) — \(refused)")
        }

        // A balance follows the token chart's period where it names none, and a chart stays at a day: either spelled out
        // is what the block covers anyway.
        layout.discard()
        let following = try result(MCPTools.draftHUD, [
            "tokens": .object(["period": .string("week")]),
            "blocks": .object(["api:DeepSeek": .object(["period": .string("week")]),
                               "Claude": .object(["widget": .string("chart"), "period": .string("day")])]),
        ])
        XCTAssertEqual(layout.draft, PanelLayout(blocks: ["Claude": PanelLayout.Block(widget: .chart)], tokens: PanelLayout.Tokens(period: .week)))
        XCTAssertEqual(try block("api:DeepSeek", in: following)["period"], .string("week"))
        XCTAssertEqual(try block("Claude", in: following)["period"], .string("day"))
    }
}
