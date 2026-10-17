import AgentHUDSupport
import XCTest
@testable import AgentHUDCore

/// A layout's sessions and tokens, their period and their models among them: their JSON, what reading them forgives, when
/// two are the same, what an agent is told it cannot list, and the plan they resolve to.
final class PanelLayoutFiltersTests: XCTestCase {
    private func decode(_ json: String) throws -> PanelLayout {
        try JSONDecoder().decode(PanelLayout.self, from: Data(json.utf8))
    }

    private func encode(_ layout: PanelLayout) throws -> String {
        String(decoding: try RecordCoding.encoder().encode(layout), as: UTF8.self)
    }

    // MARK: Coding

    func testTheDocumentedShapeReadsAndWritesBack() throws {
        let json = #"{"sessions":{"agents":["Codex"],"limit":3,"states":["waiting","running"]},"tokens":{"agents":["Claude"],"period":"week"}}"#
        let layout = PanelLayout(sessions: PanelLayout.Sessions(limit: 3, agents: ["Codex"], states: [.waiting, .running]),
                                 tokens: PanelLayout.Tokens(period: .week, agents: ["Claude"]))
        XCTAssertEqual(try decode(json), layout)
        XCTAssertEqual(try encode(layout), json, "keys are sorted and nothing unset is written")
        XCTAssertEqual(try encode(try decode(json)), json)
    }

    func testTheDefaultsAreNotWritten() throws {
        XCTAssertEqual(try encode(PanelLayout(sessions: PanelLayout.Sessions(limit: 5), tokens: PanelLayout.Tokens(period: .day))), "{}")
        XCTAssertEqual(try encode(PanelLayout(sessions: PanelLayout.Sessions(states: PanelSessionState.allCases))), "{}",
                       "every state is what no state means")
        XCTAssertTrue(PanelLayout(sessions: PanelLayout.Sessions(limit: 5, states: PanelSessionState.allCases)).isDefault)
        XCTAssertEqual(try encode(PanelLayout(sessions: PanelLayout.Sessions(limit: 10))), #"{"sessions":{"limit":10}}"#)
        XCTAssertEqual(try encode(PanelLayout(tokens: PanelLayout.Tokens(period: .month))), #"{"tokens":{"period":"month"}}"#)
        XCTAssertEqual(try decode("{}").sessions.rowLimit, 5)
        XCTAssertEqual(try decode("{}").tokens.chartPeriod, .day)
    }

    /// A stored layout from a newer build still loads: what this build cannot read in them counts as not given.
    func testStoredSessionsAndTokensAreReadForgivingly() throws {
        let layout = try decode("""
            {"sessions": {"limit": 11, "agents": ["Claude", 4], "states": ["idle", "ended"], "project": "x"},
             "tokens": {"period": "year", "agents": "Codex"}}
            """)
        XCTAssertEqual(layout.sessions, PanelLayout.Sessions(agents: ["Claude"], states: [.ended]))
        XCTAssertEqual(layout.tokens, PanelLayout.Tokens())
        for limit in ["0", "2.5", "\"3\"", "-1", "true"] {
            XCTAssertNil(try decode(#"{"sessions": {"limit": \#(limit)}}"#).sessions.limit, limit)
        }
        XCTAssertEqual(try decode(#"{"sessions": {"limit": 7.0}}"#).sessions.limit, 7)
        XCTAssertEqual(try decode(#"{"sessions": [1], "tokens": "week"}"#), PanelLayout(), "neither is an object")
    }

    func testTheSameFilterIsTheSameLayout() {
        let spelled = PanelLayout(sessions: PanelLayout.Sessions(limit: 5, agents: ["DeepSeek", "Codex", "ChatGPT", ""],
                                                                 states: [.ended, .waiting, .ended]),
                                  tokens: PanelLayout.Tokens(period: .day, agents: ["Codex", "Claude", "ChatGPT"]))
        let plain = PanelLayout(sessions: PanelLayout.Sessions(agents: ["Codex", "DeepSeek"], states: [.waiting, .ended]),
                                tokens: PanelLayout.Tokens(agents: ["Claude", "Codex"]))
        XCTAssertEqual(spelled.normalized(), plain.normalized(),
                       "a filter has no order, and Codex and ChatGPT are one mark, named as first given")
        XCTAssertEqual(spelled.normalized().sessions, PanelLayout.Sessions(agents: ["Codex", "DeepSeek"], states: [.waiting, .ended]))
        XCTAssertEqual(PanelLayout(sessions: PanelLayout.Sessions(limit: 0)).normalized(), PanelLayout())
    }

    // MARK: What an agent is told

    func testAnAgentWithNoSessionsOrTokensNowIsAnIssueUnlessTheLayoutHoldsIt() {
        let layout = PanelLayout(sessions: PanelLayout.Sessions(agents: ["ChatGPT", "Grok", ""]),
                                 tokens: PanelLayout.Tokens(agents: ["Claude", "Gemini"]))
        let issues = layout.filterIssues(sessionAgents: ["Claude", "Codex"], tokenAgents: ["Claude", "Codex"])
        XCTAssertEqual(issues.map(\.path), ["sessions.agents[1]", "tokens.agents[1]"], "ChatGPT is Codex's mark")
        XCTAssertEqual(issues.first?.message, "\"Grok\" has no session now. Agents with sessions now: \"Claude\", \"Codex\".")
        XCTAssertEqual(issues.last?.message,
                       "\"Gemini\" has no model in the token chart now. Agents in the chart now: \"Claude\", \"Codex\".")
        XCTAssertEqual(layout.filterIssues(sessionAgents: [], tokenAgents: []).map(\.message).last,
                       "\"Gemini\" has no model in the token chart now. The chart has no agent's tokens now.")

        let held = PanelLayout(sessions: PanelLayout.Sessions(agents: ["Grok CLI"]), tokens: PanelLayout.Tokens(agents: ["Gemini"]))
        XCTAssertTrue(layout.filterIssues(sessionAgents: ["Claude", "Codex"], tokenAgents: ["Claude"], holding: [held]).isEmpty,
                      "what a layout holds for an agent that is away may come back, matched by mark")
    }

    /// A project is named by its id, or by a path inside it, as written, through a link or as Git places a worktree, the
    /// innermost project first; a path or a name that names none is an issue, by its place in the list.
    func testAProjectIsNamedByItsIdOrByAPathInsideIt() throws {
        let fixture = try Worktree()
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let main = SessionProject.directory(fixture.main.path), plain = SessionProject.directory(fixture.plain.path)
        let offered = [main, plain, .directory("/agent-hud-tests/outer"), .directory("/agent-hud-tests/outer/inner"), .unassigned]
        let entries: [JSONValue] = [
            .string(main.id), .string(fixture.worktree.appendingPathComponent("Sources").path),
            .string(fixture.link.appendingPathComponent("docs").path), .string(fixture.plain.path + "/"),
            .string("plain"), .string("/agent-hud-tests/other"), .string("unassigned"), .integer(3), .string(""),
            .string("relative/path"), .string("/agent-hud-tests/outer/inner/x"), .string("/agent-hud-tests/outer/x"),
        ]
        let match = PanelLayout.projectMatch(.array(entries), among: offered)
        XCTAssertEqual(match.ids, [main.id, main.id, plain.id, plain.id, "unassigned", "/agent-hud-tests/outer/inner",
                                   "/agent-hud-tests/outer"])
        XCTAssertEqual(match.paths.map(\.given), [fixture.worktree.appendingPathComponent("Sources").path,
                                                   fixture.link.appendingPathComponent("docs").path, fixture.plain.path + "/",
                                                   "/agent-hud-tests/outer/inner/x", "/agent-hud-tests/outer/x"],
                       "every entry not given by its id, which the reply tells")
        XCTAssertEqual(match.issues.map(\.path), ["sessions.projects[4]", "sessions.projects[5]", "sessions.projects[9]"],
                       "an entry that is no string or is empty is the reading's to tell")
        let listed = "Projects with sessions now: \(PanelLayout.quoted(offered.map(\.id)))."
        XCTAssertEqual(match.issues.map(\.message), [
            "\"plain\" is a project's name, not its id. Name it by its id: \"\(plain.id)\".",
            "\"/agent-hud-tests/other\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. \(listed)",
            "\"relative/path\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. \(listed)",
        ])
    }

    func testAProjectWithNoSessionsIsAnIssueUnlessTheLayoutHoldsIt() {
        let home = NSHomeDirectory()
        let match = PanelLayout.projectMatch(.array([.string("~/work/alpha/Sources"), .string("unassigned")]),
                                             among: [.directory(home + "/work/alpha")])
        XCTAssertEqual(match.ids, ["~/work/alpha"], "a path written with ~ names the project too")
        XCTAssertEqual(match.issues.map(\.message), ["\"unassigned\" is no project's id from get_hud's "
                                                    + "sessions.offeredProjects, nor a path inside one. Projects with sessions now: "
                                                    + "\"~/work/alpha\"."])
        XCTAssertEqual(PanelLayout.projectMatch(.array([.string("/work/x")]), among: []).issues.map(\.message),
                       ["\"/work/x\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. "
                        + "No project has a session now."])

        // Saved while it had sessions; a layout that was read can be sent back with it, and a path inside it names it.
        let held = PanelLayout(sessions: PanelLayout.Sessions(projects: ["/agent-hud-tests/away", "unassigned"]))
        let kept = PanelLayout.projectMatch(.array([.string("/agent-hud-tests/away"), .string("/agent-hud-tests/away/sub"),
                                                    .string("unassigned")]), among: [], holding: [held])
        XCTAssertTrue(kept.issues.isEmpty, "\(kept.issues)")
        XCTAssertEqual(kept.ids, ["/agent-hud-tests/away", "/agent-hud-tests/away", "unassigned"])
        XCTAssertTrue(PanelLayout.projectMatch(.null, among: []).ids.isEmpty)
    }

    // MARK: The plan

    func testThePlanResolvesTheFiltersAndTheDefaultViewNarrowsNothing() {
        let plain = PanelLayoutPlan(layout: PanelLayout(), blocks: [])
        XCTAssertEqual(plain.sessions.limit, 5)
        XCTAssertEqual(plain.tokens.period, .day)
        XCTAssertFalse(plain.tokens.narrows)
        XCTAssertTrue(plain.sessions.keeps(vendor: nil, state: .ended))

        let plan = PanelLayoutPlan(layout: PanelLayout(
            sessions: PanelLayout.Sessions(limit: 8, agents: ["ChatGPT", "Grok CLI"], states: [.running]),
            tokens: PanelLayout.Tokens(period: .month, agents: ["Codex"])), blocks: [])
        XCTAssertEqual(plan.sessions.limit, 8)
        XCTAssertTrue(plan.sessions.keeps(vendor: "Codex", state: .running), "matched by mark")
        XCTAssertTrue(plan.sessions.keeps(vendor: "Grok CLI", state: .running))
        XCTAssertFalse(plan.sessions.keeps(vendor: "Grok Bot", state: .running), "Grok's clients are apart")
        XCTAssertFalse(plan.sessions.keeps(vendor: "Codex", state: .waiting))
        XCTAssertFalse(plan.sessions.keeps(vendor: nil, state: .running), "a session of no agent is no listed agent's")
        XCTAssertEqual(plan.tokens.period, .month)
        let codex = AgentDescriptor(id: "codex-model:gpt", vendor: "Codex", model: "GPT", source: "", enabled: true)
        let claude = AgentDescriptor(id: "claude-model:opus", vendor: "Claude", model: "Opus", source: "", enabled: true)
        XCTAssertTrue(plan.tokens.keeps(codex))
        XCTAssertFalse(plan.tokens.keeps(claude))
        XCTAssertEqual(PanelSessionFilter(PanelLayout.Sessions(states: PanelSessionState.allCases)).states.count, 3)
    }

    // MARK: The sessions' period and the chart's models

    func testThePeriodAndTheModelsReadAndWriteBack() throws {
        let json = #"{"sessions":{"period":"week"},"tokens":{"models":["claude-model:claude-opus-5-5","codex-model:gpt-6.1-sol"]}}"#
        let layout = PanelLayout(sessions: PanelLayout.Sessions(period: .week),
                                 tokens: PanelLayout.Tokens(models: ["claude-model:claude-opus-5-5", "codex-model:gpt-6.1-sol"]))
        XCTAssertEqual(try decode(json), layout)
        XCTAssertEqual(try encode(layout), json)
        XCTAssertEqual(try encode(PanelLayout(sessions: PanelLayout.Sessions(period: .day))), "{}", "a day is the default view's")
        XCTAssertEqual(try decode("{}").sessions.listedPeriod, .day)
        XCTAssertEqual(try decode(#"{"sessions": {"period": "day"}}"#).sessions.period, .day)

        // A stored layout from a newer build still loads.
        let stored = try decode(#"{"sessions": {"period": "30d"}, "tokens": {"models": ["claude-model:a", 4, null]}}"#)
        XCTAssertNil(stored.sessions.period, "a period this build does not know is not given")
        XCTAssertEqual(stored.tokens.models, ["claude-model:a"], "an id that is not a string is left out")
        XCTAssertTrue(try decode(#"{"tokens": {"models": "claude-model:a"}}"#).isDefault)
    }

    func testTheSameModelsAreTheSameLayout() {
        let spelled = PanelLayout(tokens: PanelLayout.Tokens(models: ["codex-model:b", "claude-model:a", "codex-model:b", ""]))
        XCTAssertEqual(spelled.normalized().tokens.models, ["claude-model:a", "codex-model:b"], "a filter has no order")
        XCTAssertEqual(PanelLayout(tokens: PanelLayout.Tokens(models: [""])).normalized(), PanelLayout())
        XCTAssertEqual(PanelLayout(sessions: PanelLayout.Sessions(period: .day)).normalized(), PanelLayout())
    }

    func testTheSessionPeriodsReachAsFarBackAsSessionsAreReported() {
        XCTAssertEqual(PanelSessionPeriod.allCases.map(\.rawValue), ["day", "week"], "named as the token chart's periods")
        XCTAssertEqual(PanelSessionPeriod.allCases.map(\.duration), [86400, 7 * 86400] as [TimeInterval])
        XCTAssertEqual(PanelSessionPeriod.week.duration, SessionWindow.recency, "no older session is reported")
        XCTAssertEqual(PanelSessionPeriod.day.duration, PanelTokenPeriod.day.duration)
        XCTAssertEqual(PanelSessionPeriod.week.duration, PanelTokenPeriod.week.duration)
    }

    /// A layout saved by the build before, whose rows reached back from an hour to seven days, loads with the period that
    /// covers most of its own: a week for three days and seven, a day for the rest.
    func testThePeriodsOfTheBuildBeforeAreReadAsTheDayOrTheWeek() throws {
        for (name, period) in [("1h", PanelSessionPeriod.day), ("2h", .day), ("6h", .day), ("12h", .day), ("24h", .day),
                               ("3d", .week), ("7d", .week), ("day", .day), ("week", .week)] {
            XCTAssertEqual(try decode(#"{"sessions": {"period": "\#(name)", "limit": 7}}"#).sessions,
                           PanelLayout.Sessions(limit: 7, period: period), name)
            XCTAssertEqual(PanelSessionPeriod(stored: name), period, name)
        }
        XCTAssertEqual(try encode(try decode(#"{"sessions": {"period": "3d"}}"#)), #"{"sessions":{"period":"week"}}"#,
                       "written back by this build's name")
        XCTAssertTrue(try decode(#"{"sessions": {"period": "6h"}}"#).isDefault, "a day is the default view's")
        for name in ["30d", "month", "1d", ""] {
            XCTAssertNil(PanelSessionPeriod(stored: name), name)
            XCTAssertNil(try decode(#"{"sessions": {"period": "\#(name)"}}"#).sessions.period, name)
        }
    }

    // MARK: The sessions' order and projects

    func testTheSortAndTheProjectsReadAndWriteBack() throws {
        let json = #"{"sessions":{"projects":["/work/beta","unassigned","~/work/alpha"],"sort":"date"}}"#
        let layout = PanelLayout(sessions: PanelLayout.Sessions(sort: .date, projects: ["/work/beta", "unassigned", "~/work/alpha"]))
        XCTAssertEqual(try decode(json), layout)
        XCTAssertEqual(try encode(layout), json, "keys are sorted and nothing unset is written")
        XCTAssertEqual(try encode(PanelLayout(sessions: PanelLayout.Sessions(sort: .active, projects: []))), "{}",
                       "the active order and every project are the default view's")
        XCTAssertEqual(try decode("{}").sessions.rowSort, .active)
        XCTAssertEqual(PanelSessionSort.allCases.map(\.rawValue), ["active", "date"])

        // A stored layout from a newer build still loads.
        let stored = try decode(#"{"sessions": {"sort": "title", "projects": ["~/work/alpha", 3, null, ""]}}"#)
        XCTAssertNil(stored.sessions.sort, "a sort this build does not know is not given")
        XCTAssertEqual(stored.sessions.projects, ["~/work/alpha", ""], "an id that is not a string is left out")
        XCTAssertEqual(stored.normalized().sessions, PanelLayout.Sessions(projects: ["~/work/alpha"]), "and an empty one too")
        XCTAssertTrue(try decode(#"{"sessions": {"projects": "~/work/alpha", "sort": 1}}"#).isDefault)
    }

    func testTheSameProjectsAreTheSameLayout() {
        let spelled = PanelLayout(sessions: PanelLayout.Sessions(sort: .active, projects: ["~/b", "unassigned", "~/a", "~/b", ""]))
        XCTAssertEqual(spelled.normalized().sessions, PanelLayout.Sessions(projects: ["unassigned", "~/a", "~/b"]),
                       "a filter has no order")
        XCTAssertEqual(PanelLayout(sessions: PanelLayout.Sessions(projects: [""])).normalized(), PanelLayout())
        XCTAssertEqual(PanelLayout(sessions: PanelLayout.Sessions(sort: .active)).normalized(), PanelLayout())
        XCTAssertNotEqual(PanelLayout(sessions: PanelLayout.Sessions(sort: .date)).normalized(), PanelLayout())
    }

    func testThePlanResolvesTheSortAndTheProjects() {
        let plain = PanelLayoutPlan(layout: PanelLayout(), blocks: []).sessions
        XCTAssertEqual(plain.sort, .active)
        XCTAssertTrue(plain.keeps(project: { .unassigned }))
        var asked = false
        XCTAssertTrue(plain.keeps(project: { asked = true; return .unassigned }))
        XCTAssertFalse(asked, "a filter that names no project asks for none, which would read Git's pointers")

        let plan = PanelLayoutPlan(layout: PanelLayout(sessions: PanelLayout.Sessions(
            sort: .date, projects: ["/work/alpha", "unassigned", ""])), blocks: []).sessions
        XCTAssertEqual(plan.sort, .date)
        XCTAssertTrue(plan.keeps(project: { .directory("/work/alpha") }))
        XCTAssertTrue(plan.keeps(project: { .unassigned }))
        XCTAssertFalse(plan.keeps(project: { .directory("/work/beta") }))
        XCTAssertFalse(plan.keeps(project: { .directory("/work/alpha/sub") }), "a session's project is matched by its id")
        XCTAssertNotEqual(plan, plain)
    }

    /// A project's id is its folder written with ~, the same in every language, or `unassigned`, and names it back.
    func testAProjectsIdIsItsFolderWrittenWithATilde() {
        let home = NSHomeDirectory()
        XCTAssertEqual(SessionProject.directory(home + "/work/alpha").id, "~/work/alpha")
        XCTAssertEqual(SessionProject.directory("/work/alpha").id, "/work/alpha")
        XCTAssertEqual(SessionProject.unassigned.id, "unassigned")
        L10n.setLanguage(.zhHans)
        defer { L10n.setLanguage(.system) }
        XCTAssertEqual(SessionProject.unassigned.id, "unassigned", "the label is the language's, the id is not")
        for project in [SessionProject.directory(home + "/work/alpha"), .directory("/work/alpha"), .unassigned] {
            XCTAssertEqual(SessionProject(id: project.id), project)
        }
        XCTAssertEqual(SessionProject.ordered([.unassigned, .directory("/work/b"), .directory("/work/a10"), .directory("/work/a9")]),
                       [.directory("/work/a9"), .directory("/work/a10"), .directory("/work/b"), .unassigned],
                       "by label as Finder orders names, the sessions of no folder last")
    }

    /// A model the chart has no tokens of now is an issue, unless a layout holds it, and so is one of an agent the
    /// layout's agents leave out, which the chart would not draw.
    func testAModelTheChartCannotDrawIsAnIssueUnlessTheLayoutHoldsIt() {
        let opus = AgentDescriptor(id: "claude-model:opus", vendor: "Claude", model: "Opus", source: "", enabled: true)
        let gpt = AgentDescriptor(id: "codex-model:gpt", vendor: "Codex", model: "GPT", source: "", enabled: true)
        let layout = PanelLayout(tokens: PanelLayout.Tokens(agents: ["Claude"],
                                                            models: ["claude-model:opus", "codex-model:gpt", "gemini-model:pro", ""]))
        let issues = layout.filterIssues(sessionAgents: [], tokenAgents: ["Claude", "Codex"], tokenModels: [opus, gpt])
        XCTAssertEqual(issues.map(\.path), ["tokens.models[1]", "tokens.models[2]"])
        XCTAssertEqual(issues.first?.message, "\"codex-model:gpt\" is Codex's, and tokens.agents leaves Codex out, so the chart "
                       + "would not draw it. Name \"Codex\" in tokens.agents too, or name no agent.")
        XCTAssertEqual(issues.last?.message, "\"gemini-model:pro\" is no model in the token chart now. Models in the chart now: "
                       + "\"claude-model:opus\", \"codex-model:gpt\".")
        XCTAssertEqual(PanelLayout(tokens: PanelLayout.Tokens(models: ["x"])).filterIssues(sessionAgents: [], tokenAgents: [])
            .map(\.message), ["\"x\" is no model in the token chart now. The chart has no model's tokens now."])
        XCTAssertTrue(PanelLayout(tokens: PanelLayout.Tokens(agents: ["ChatGPT"], models: ["codex-model:gpt"]))
            .filterIssues(sessionAgents: [], tokenAgents: ["Codex"], tokenModels: [opus, gpt]).isEmpty, "matched by mark")

        let held = PanelLayout(tokens: PanelLayout.Tokens(models: ["gemini-model:pro"]))
        XCTAssertEqual(layout.filterIssues(sessionAgents: [], tokenAgents: ["Claude", "Codex"], tokenModels: [opus, gpt], holding: [held])
            .map(\.path), ["tokens.models[1]"], "a model a layout holds may come back while it spends no tokens")
    }

    func testThePlanResolvesThePeriodAndTheModels() {
        let opus = AgentDescriptor(id: "claude-model:opus", vendor: "Claude", model: "Opus", source: "", enabled: true)
        let sonnet = AgentDescriptor(id: "claude-model:sonnet", vendor: "Claude", model: "Sonnet", source: "", enabled: true)
        let gpt = AgentDescriptor(id: "codex-model:gpt", vendor: "Codex", model: "GPT", source: "", enabled: true)
        XCTAssertEqual(PanelLayoutPlan(layout: PanelLayout(), blocks: []).sessions.period, .day)
        let plan = PanelLayoutPlan(layout: PanelLayout(sessions: PanelLayout.Sessions(period: .week),
                                                       tokens: PanelLayout.Tokens(models: ["claude-model:opus"])), blocks: [])
        XCTAssertEqual(plan.sessions.period, .week)
        XCTAssertTrue(plan.tokens.narrows)
        XCTAssertTrue(plan.tokens.keeps(opus))
        XCTAssertFalse(plan.tokens.keeps(sonnet))
        let both = PanelTokenFilter(PanelLayout.Tokens(agents: ["Codex"], models: ["claude-model:opus", "codex-model:gpt"]))
        XCTAssertFalse(both.keeps(opus), "the agents and the models both apply")
        XCTAssertTrue(both.keeps(gpt))
        XCTAssertNotEqual(PanelTokenFilter(PanelLayout.Tokens(models: ["claude-model:opus"])), PanelTokenFilter(),
                          "another chart, whose picker starts again from the layout's period")
    }

    func testThePeriodsAreTheStatisticsRangesInTheirOwnColumns() {
        XCTAssertEqual(PanelTokenPeriod.allCases.map(\.label), ["24h", "7d", "30d"])
        XCTAssertEqual(PanelTokenPeriod.allCases.map(\.duration), [86400, 7 * 86400, 30 * 86400] as [TimeInterval])
        XCTAssertEqual(PanelTokenPeriod.allCases.map(\.bucketSize), [.minutes15, .day1, .day1])
        XCTAssertEqual(PanelTokenPeriod.allCases.map(\.range), [.hours24, .days7, .days30])
    }

    /// A repository with a worktree, as Git leaves their pointers, beside a folder outside Git and a link to it, in a
    /// folder of its own; Git itself is not run.
    private struct Worktree {
        let directory: URL
        let main: URL
        let worktree: URL
        let plain: URL
        let link: URL

        init() throws {
            let files = FileManager.default
            directory = files.temporaryDirectory.appendingPathComponent("HUD Projects \(UUID().uuidString)").resolvingSymlinksInPath()
            main = directory.appendingPathComponent("main")
            worktree = directory.appendingPathComponent("tree")
            plain = directory.appendingPathComponent("plain")
            link = directory.appendingPathComponent("link")
            let pointers = main.appendingPathComponent(".git/worktrees/tree")
            for folder in [pointers, worktree.appendingPathComponent("Sources"), plain.appendingPathComponent("docs")] {
                try files.createDirectory(at: folder, withIntermediateDirectories: true)
            }
            try "../..\n".write(to: pointers.appendingPathComponent("commondir"), atomically: true, encoding: .utf8)
            try "gitdir: \(pointers.path)\n".write(to: worktree.appendingPathComponent(".git"), atomically: true, encoding: .utf8)
            try files.createSymbolicLink(at: link, withDestinationURL: plain)
        }
    }
}
