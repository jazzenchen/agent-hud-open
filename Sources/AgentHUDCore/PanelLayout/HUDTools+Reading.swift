import AgentHUDSupport
import Foundation

/// What `get_hud` tells an agent, and what every tool but `get_hud_screenshot` answers with.
extension HUDTools {
    // MARK: Reading

    /// The screen argument that names a scope.
    static func name(of scope: PanelLayoutScope) -> String {
        guard case .screen(let key) = scope else { return "all" }
        return key
    }

    /// Whether the panel of a scope shows a draft and the bar that says so: the shared layout's own, or for a screen the
    /// one its bar saves, and the app's draft, which every panel's bar shows.
    func unsaved(_ scope: PanelLayoutScope) -> Bool {
        layout.hasDraft(on: scope.screenKey)
    }

    /// The panel as an agent reads it: which screen, the layout in effect there, with a screen's style draft in its
    /// hud, and the style it sets, its width among it, the panel's height on that display and whether it scrolls there,
    /// whether the quota blocks show reset countdowns, every section and every block the panel has, shown or hidden,
    /// with the span each takes there, its accounts and the readings of its rows in the order shown, the session rows
    /// and the token chart as the layout filters them, the attached screens, and what the app's own settings draw now,
    /// with whether the app's draft waits.
    /// - proposed: whether the note answers a proposal, after which it says which blocks the layout gives half a line
    ///   take the whole one there, and why.
    func panel(_ scope: PanelLayoutScope, among attached: [PanelScreen], note: String? = nil,
               proposed: Bool = false) -> [String: JSONValue] {
        let effective = effective(scope), existing = store.view.panelBlocks
        let measured = fit?(scope)
        let plan = PanelLayoutPlan(layout: effective, blocks: existing, contentWidth: measured?.contentWidth)
        let sections = plan.sections + PanelSection.allCases.filter { !plan.sections.contains($0) }
        let shown = Set(plan.blocks.map(\.key))
        let drawn = plan.blocks + existing.filter { !shown.contains($0.key) }.map { PanelBlockPlan($0, in: effective) }
        let style: [String: JSONValue] = ["spacing": .string(plan.style.spacing.rawValue), "separators": .bool(plan.style.separators),
                                          "width": .integer(Int64(plan.style.panelWidth))]
        var result: [String: JSONValue] = [
            "screen": .string(Self.name(of: scope)),
            "revision": .integer(revision),
            "unsaved": .bool(unsaved(scope)),
            "hasPrevious": .bool(layout.canRevert(scope)),
            "layout": layoutJSON(scope),
            "style": .object(style),
            "countdown": .bool(effective.countdown),
            "sections": .array(sections.map { section in
                .object(["key": .string(section.rawValue), "shown": .bool(plan.sections.contains(section))])
            }),
            "blocks": .array(drawn.map { block(of: $0, shown: shown.contains($0.key)) }),
            "sessions": sessions(plan.sessions, listing: effective.sessions.normalized()),
            "tokens": tokens(plan.tokens, listing: effective.tokens.normalized()),
            "screens": .array(attached.map(listing)),
            "app": .object(appListing()),
            "appUnsaved": .bool(layout.appDraft != nil),
        ]
        if case .screen(let key) = scope { result["usesSharedLayout"] = .bool(layout.usesShared(key)) }
        if let measured {
            result["height"] = .integer(Int64(measured.height.rounded()))
            result["scrolls"] = .bool(measured.scrolls)
        }
        if let note { result["note"] = .string(proposed ? note + Self.wholeLines(plan, asked: effective) : note) }
        return result
    }

    /// What says which blocks the layout gives half a line take the whole one, and why: a sentence after a note, or
    /// nothing where every such block shares a line.
    private static func wholeLines(_ plan: PanelLayoutPlan, asked layout: PanelLayout) -> String {
        let whole = plan.blocks.filter { block in
            layout.blocks[block.key]?.span == .half && block.kind.takesHalf(under: block.widget) && block.span == .full
        }
        guard !whole.isEmpty else { return "" }
        let names = PanelLayout.quoted(whole.map(\.key))
        let reason = plan.halvesFit ? "no half block is left next to \(whole.count == 1 ? "it" : "each") to share a line with"
            : "the panel is too narrow here for two halves side by side"
        return " \(names) \(whole.count == 1 ? "takes" : "take") the whole line: \(reason)."
    }

    /// One attached screen as an agent reads it: its key and role, how it presents the HUD, which layout its panel
    /// draws, the panel's width there and that of one agent's own panel on a dock that separates its agents, whether
    /// the panel shows a draft, its resting HUD's style in effect, the look it draws now and whether a style draft
    /// waits, the agents its resting HUD can show now, those it shows under its layout, and whether an agent's edit
    /// holds its panel open (`holdListing(_:)`). The width of an agent's own panel is told whether or not the screen
    /// separates its agents, so that a proposal that turns that on can be weighed first.
    private func listing(_ screen: PanelScreen) -> JSONValue {
        let drawn = layout.effective(for: screen.key)
        let order = HUDAgentOrder(listed: drawn.hud.agents, available: screen.agents)
        let listed: [String: JSONValue] = [
            "key": .string(screen.key), "name": .string(screen.name), "mode": .string(screen.modeName),
            "edge": .string(screen.edge.rawValue), "hasNotch": .bool(screen.hasNotch),
            "layout": .string(layout.usesShared(screen.key) ? "shared" : "own"),
            "width": .integer(Int64(drawn.style.panelWidth)),
            "agentPanelWidth": .integer(Int64(drawn.style.agentPanelWidth)),
            "unsaved": .bool(layout.hasDraft(on: screen.key)),
            "hud": styleInEffect(of: screen),
            "look": lookInEffect(of: screen),
            "styleUnsaved": .bool(layout.hasStyleDraft(screen.key)),
            "agents": .array(screen.agents.map(JSONValue.string)),
            "shows": .array(order.arrange(screen.agents) { $0 }.map(JSONValue.string)),
        ]
        return .object(listed.merging(holdListing(screen.key)) { $1 })
    }

    /// One block as an agent reads it: how it is drawn, what it offers, its accounts and its readings. The countdown,
    /// the columns, the ring size and the period are told only where the block has them to set, and the columns and the
    /// span as the panel sets the block in its line. A quota block's rows are told in the order it draws them, account
    /// by account, each account's windows in the block's order, each with the account it belongs to.
    private func block(of plan: PanelBlockPlan, shown: Bool) -> JSONValue {
        var block: [String: JSONValue] = [
            "key": .string(plan.key), "kind": .string(plan.kind.rawValue), "shown": .bool(shown),
            "widget": .string(plan.widget.rawValue),
            "widgets": .array(plan.kind.widgets.map { .string($0.rawValue) }),
            "measures": .array(plan.measures.map { .string($0.rawValue) }),
            "offeredMeasures": .array(plan.kind.measures(under: plan.widget).map { .string($0.rawValue) }),
        ]
        if plan.kind.takesCountdown { block["countdown"] = .bool(plan.countdown) }
        if plan.kind.takesColumns(under: plan.widget) { block["columns"] = .integer(Int64(plan.columns)) }
        if plan.kind.takesRingSize(under: plan.widget) { block["ringSize"] = .string(plan.ringSize.rawValue) }
        if plan.kind.takesPeriod(under: plan.widget) { block["period"] = .string(plan.period.rawValue) }
        block["span"] = .string(plan.span.rawValue)
        switch plan.kind {
        case .quota:
            block["name"] = .string(VendorCatalog.name(plan.key))
            let sections = store.accountSections(store.rowGroups.first { $0.vendor == plan.key }?.rows ?? [])
            // Rows of no account come under an empty id, which no layout names.
            block["accounts"] = .array(sections.filter { !$0.id.isEmpty }.map { section in
                var entry: [String: JSONValue] = [
                    "id": .string(section.id), "current": .bool(section.isCurrent), "shown": .bool(plan.shows(account: section.id)),
                ]
                if let account = section.account {
                    entry["name"] = .string(account.displayName)
                    if let label = account.planLabel { entry["plan"] = .string(label) }
                }
                return .object(entry)
            })
            block["rows"] = .array(sections.flatMap { section in
                plan.ordered(section.rows, key: \.agent.windowKey).map { row in
                    reading(of: row, account: section.id.isEmpty ? nil : section.id,
                            shown: !plan.hiddenRows.contains(row.agent.windowKey) && plan.shows(account: section.id))
                }
            })
        case .billing:
            let accounts = store.enabledBilling.filter { $0.panelBlockKey == plan.key }
            block["name"] = .string(accounts.first?.displayName ?? plan.key)
            block["accounts"] = .array(accounts.map { account in
                .object(["id": .string(account.id), "name": .string(account.displayName),
                         "shown": .bool(plan.shows(account: account.id))])
            })
            block["balances"] = .array(accounts.flatMap { account in
                account.balances.map { balance in self.balance(balance, of: account, over: plan.period) }
            })
        }
        return .object(block)
    }

    /// One API balance's reading: the account it belongs to, by the id a layout's `accounts` names it by, its amount,
    /// level, state and the time it runs out at its own fall (`runsOutAt`, `AccountBalance.runsOutAt`), and over its
    /// block's period what the measures beside it read: the estimated cost of the requests recorded (`cost`), that cost
    /// per day (`burnPerDay`), and when the amount runs out at that pace (`burnRunsOutAt`), which a reading that shows no
    /// level has not, as in the panel. Amounts are written as decimals in the balance's currency, a pace to the
    /// millionth; nothing is told that its recorded requests leave unpriced.
    private func balance(_ balance: AccountBalance, of account: APIBilling, over period: PanelTokenPeriod) -> JSONValue {
        var entry: [String: JSONValue] = ["account": .string(account.id), "currency": .string(balance.currency),
                                          "amount": .string("\(balance.total)"),
                                          "state": .string(Self.state(store.view.assessment(of: account)))]
        if let level = store.balanceLevel(balance, billing: account) { entry["level"] = .string(level.rawValue) }
        if let date = balance.runsOutAt { entry["runsOutAt"] = .string(Self.time(date)) }
        if let spend = store.balanceSpend(account, currency: balance.currency, period: period) {
            entry["cost"] = .string("\(spend.cost)")
            entry["burnPerDay"] = .string(Self.millionths(spend.perDay))
            if let date = store.balanceRunsOut(balance, billing: account, at: spend) { entry["burnRunsOutAt"] = .string(Self.time(date)) }
        }
        return .object(entry)
    }

    /// The session rows as an agent reads them: the limit, the period, the order, the agents, the projects and the
    /// states in effect, the agents that have sessions now and the projects those sessions belong to, each by its id and
    /// its name, how many rows are shown and how many more the filter lists, and how many sessions of the period each
    /// state has before the agents, projects and states. Counts, agents' names and projects only: no title, prompt or
    /// session id. A project is the one the store keeps for the report; nothing is read for it but Git's pointers, once.
    private func sessions(_ filter: PanelSessionFilter, listing listed: PanelLayout.Sessions) -> JSONValue {
        let view = store.view, rows = store.panelSessions(filter)
        let states = filter.states.isEmpty ? PanelSessionState.allCases : PanelSessionState.allCases.filter(filter.states.contains)
        return .object([
            "limit": .integer(Int64(filter.limit)),
            "period": .string(filter.period.rawValue),
            "sort": .string(filter.sort.rawValue),
            "agents": .array(listed.agents.map(JSONValue.string)),
            "projects": .array(listed.projects.map(JSONValue.string)),
            "states": .array(states.map { .string($0.rawValue) }),
            "offeredAgents": .array(view.panelSessionAgents.map(JSONValue.string)),
            "offeredProjects": .array(store.panelSessionProjects.map { project in
                .object(["id": .string(project.id), "name": .string(project.title)])
            }),
            "shown": .integer(Int64(rows.shown.count)),
            "more": .integer(Int64(rows.more)),
            "byState": .object(Dictionary(uniqueKeysWithValues: PanelSessionState.allCases.map {
                ($0.rawValue, JSONValue.integer(Int64(rows.counts[$0] ?? 0)))
            })),
        ])
    }

    /// The token chart as an agent reads it: the period it opens with, the agents and the models in effect, the agents
    /// and the models it can draw now, each model by its id with its name and its agent, and the tokens it counts over
    /// that period. A model is told from the report the store holds; nothing is read for it.
    private func tokens(_ filter: PanelTokenFilter, listing listed: PanelLayout.Tokens) -> JSONValue {
        let columns = store.hudTokenColumns(consumerIDs: store.panelTokenConsumers(filter), period: filter.period)
        let view = store.view
        return .object([
            "period": .string(filter.period.rawValue),
            "agents": .array(listed.agents.map(JSONValue.string)),
            "models": .array(listed.models.map(JSONValue.string)),
            "offeredAgents": .array(view.panelTokenAgents.map(JSONValue.string)),
            "offeredModels": .array(view.panelTokenModels.map { model in
                .object(["id": .string(model.id), "name": .string(model.name), "agent": .string(model.displayVendor)])
            }),
            "total": .integer(Int64(columns.reduce(0) { $0 + $1.total })),
        ])
    }

    /// One quota window's reading: the id of the account it belongs to, where it belongs to one, numbers and times. Its
    /// name is the window's as the panel writes it, in the application's language: its provider worded it when it was
    /// read, and it is how the person who looks at the panel will call the row. A reading that shows no level has no
    /// burn rate, run-out time or token rate, as in the panel.
    private func reading(of row: AgentRow, account: String?, shown: Bool) -> JSONValue {
        var reading: [String: JSONValue] = [
            "key": .string(row.agent.windowKey), "name": .string(row.agent.name), "shown": .bool(shown),
            "state": .string(Self.state(row.assessment)),
        ]
        if let account { reading["account"] = .string(account) }
        if let used = row.usedPct { reading["usedPercent"] = .number(Self.tenths(used)) }
        if let level = row.level { reading["level"] = .string(level.rawValue) }
        if let reset = row.resetAt { reading["resetsAt"] = .string(Self.time(reset)) }
        if row.assessment.showsLevel, let rate = store.report?.insightsByAgent[row.id]?.burnRatePctPerHour {
            reading["burnPercentPerHour"] = .number(Self.tenths(rate))
        }
        if case .exhausts(let interval, beforeReset: true)? = store.view.outlook(for: row.id) {
            reading["runsOutAt"] = .string(Self.time(store.now.addingTimeInterval(interval)))
        }
        if let tokens = store.quotaTokensPerHour(for: row.id) { reading["tokensPerHour"] = .integer(Int64(tokens)) }
        return .object(reading)
    }

    /// The first thing that holds a reading back, in the order the panel weighs them; "normal" when nothing does.
    private static func state(_ assessment: ReadingAssessment) -> String {
        if !assessment.isCurrentAccount { return "other account" }
        switch assessment.status {
        case .readFailed: return "read failed"
        case .unverified: return "unverified"
        case .normal: break
        }
        if assessment.observedAt == nil { return "not read yet" }
        if !assessment.hasValue { return "no value" }
        if assessment.isResetPending { return "reset pending" }
        return assessment.isStale ? "stale" : "normal"
    }

    private static func tenths(_ value: Double) -> Double { (value * 10).rounded() / 10 }
    /// An amount of money to the millionth, the most an estimate is written with, as a decimal.
    private static func millionths(_ value: Decimal) -> String {
        var exact = value, rounded = Decimal()
        NSDecimalRound(&rounded, &exact, 6, .plain)
        return "\(rounded)"
    }
    /// A picture's scale to the hundredth below it, which never reads as more pixels than were drawn.
    static func hundredths(_ value: Double) -> Double { (value * 100).rounded(.down) / 100 }
    static func points(_ value: Double) -> String { "\(Int64(value.rounded()))" }
    private static func time(_ date: Date) -> String { date.formatted(.iso8601) }
}
