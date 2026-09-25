import AgentHUDSupport
import Foundation

/// The agents, sessions and figures shown in the design handoff.
public enum DemoData {
    public static let agents: [AgentDescriptor] = [
        AgentDescriptor(id: "claude-opus", vendor: "Claude", model: "Opus 4.5", source: L10n.sourceClaudeSessions, enabled: true),
        AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 4.5", source: L10n.sourceClaudeSessions, enabled: true),
        AgentDescriptor(id: "chatgpt", vendor: "ChatGPT", model: "GPT‑5 Plus", source: L10n.sourceBrowserAuth, enabled: true),
        AgentDescriptor(id: "codex", vendor: "Codex", model: "CLI", source: L10n.sourceCodexAppServer, enabled: true),
        AgentDescriptor(id: "antigravity", vendor: "Antigravity", model: "Agent", source: L10n.sourceNotConnected, enabled: false),
        AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceNotConnected, enabled: false),
    ]

    /// Every vendor the app ships artwork for, all switched on. `agents` is the small set the tests and
    /// snapshots are written against; this is what the running demo uses, so the HUD is shown carrying a
    /// full queue rather than the three marks a minimal set produces.
    public static let everyAgent: [AgentDescriptor] = agents.map {
        AgentDescriptor(id: $0.id, vendor: $0.vendor, model: $0.model, source: $0.source, enabled: true)
    } + [
        AgentDescriptor(id: "cursor", vendor: "Cursor", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "copilot", vendor: "GitHub Copilot", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "grok", vendor: "Grok", model: "Code", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "kimi", vendor: "Kimi", model: "K2", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "glm", vendor: "GLM", model: "Coding", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "opencode", vendor: "OpenCode", model: "CLI", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "openclaw", vendor: "OpenClaw", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "hermes", vendor: "Hermes", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "zcode", vendor: "ZCode", model: "CLI", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "codebuddy", vendor: "CodeBuddy", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "workbuddy", vendor: "WorkBuddy", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "qwen", vendor: "Qwen", model: "Code", source: L10n.sourceNotConnected, enabled: true),
        AgentDescriptor(id: "pi", vendor: "Pi", model: "Agent", source: L10n.sourceNotConnected, enabled: true),
    ]

    /// Remaining % and seconds until reset per agent.
    public static let quota: [String: (remaining: Double, resetIn: TimeInterval?, weekly: Double?)] = [
        "claude-opus": (72, 2 * 3600 + 14 * 60, 61),
        "claude-sonnet": (24, 2 * 3600 + 14 * 60, 61),
        "chatgpt": (58, 4 * 3600 + 2 * 60, 80),
        "codex": (7, 51 * 60, nil),
        "antigravity": (91, 6 * 3600 + 40 * 60, nil),
        "deepseek": (44, nil, nil),
        // Only `everyAgent` carries these; a reading each, spread across the thresholds.
        "cursor": (83, 3 * 3600 + 12 * 60, nil),
        "copilot": (12, 5 * 3600, nil),
        "grok": (66, 90 * 60, nil),
        "kimi": (38, 2 * 3600, nil),
        "glm": (95, 8 * 3600, nil),
        "opencode": (51, nil, nil),
        "openclaw": (4, 33 * 60, nil),
        "hermes": (77, 4 * 3600 + 25 * 60, nil),
        "zcode": (29, 70 * 60, nil),
        "codebuddy": (88, 6 * 3600, nil),
        "workbuddy": (19, 45 * 60, nil),
        "qwen": (47, 2 * 3600 + 30 * 60, nil),
        "pi": (60, 3 * 3600, nil),
    ]

    public static func snapshots(now: Date) -> [UsageSnapshot] {
        agents.compactMap { agent in
            guard let q = quota[agent.id] else { return nil }
            return UsageSnapshot(
                agentId: agent.id,
                remainingPct: q.remaining,
                weeklyRemainingPct: q.weekly,
                resetAt: q.resetIn.map { now.addingTimeInterval($0) },
                windowDuration: q.resetIn == nil ? nil : (agent.id == "antigravity" ? 24 : 5) * 3600,
                weeklyResetAt: q.weekly == nil ? nil : now.addingTimeInterval(3 * 86400),
                updatedAt: now
            )
        }
    }

    public static func codexResetCredits(now: Date) -> CodexResetCredits {
        CodexResetCredits(availableCount: 3, credits: [14, 27, 28].map { days in
            .init(id: "demo-reset-\(days)", expiresAt: now.addingTimeInterval(Double(days) * 86400).timeIntervalSince1970)
        })
    }

    public static func deepSeekBilling(now: Date) -> APIBilling {
        let balance = Decimal(string: "8.85")!
        return APIBilling(
            vendor: "DeepSeek",
            balances: [AccountBalance(currency: "CNY", total: balance, granted: 0, toppedUp: balance)],
            isAvailable: true, updatedAt: now,
            costs: [CostBucket(start: Date(timeIntervalSince1970: (now.timeIntervalSince1970 - 60) - (now.timeIntervalSince1970 - 60).truncatingRemainder(dividingBy: 900)),
                               amounts: ["CNY": Decimal(string: "0.01239")!])],
            sessionCosts: ["deepseek-sample": ["CNY": Decimal(string: "0.01239")!]],
            notice: nil
        )
    }

    public static func sessions(now: Date) -> [LiveSession] {
        let usage = sessionUsage(now: now)
        /// The session's own counts: its breakdown without the sub-agent.
        func own(_ id: String) -> SessionUsage.Tokens {
            guard let usage = usage[id] else { return .init() }
            let total = usage.total, helper = usage.subagents ?? .init()
            return .init(tokensIn: total.tokensIn - helper.tokensIn, tokensOut: total.tokensOut - helper.tokensOut,
                         cacheReadTokens: total.cacheReadTokens - helper.cacheReadTokens)
        }
        let s1 = own("s1"), s2 = own("s2"), s3 = own("s3"), home = FileManager.default.homeDirectoryForCurrentUser.path
        return [
            LiveSession(id: "s1", agentId: "claude-opus", task: "fix auth bug in middleware", terminal: "api-gateway",
                        startedAt: now.addingTimeInterval(-27 * 60), pctOfWindow: 6.2, tokensIn: s1.tokensIn, tokensOut: s1.tokensOut,
                        cacheReadTokens: s1.cacheReadTokens, observedAt: now, workingDirectory: home + "/work/api-gateway"),
            LiveSession(id: "s2", agentId: "codex", task: "backend server endpoints", terminal: "hud-ios",
                        startedAt: now.addingTimeInterval(-64 * 60), pctOfWindow: 3.8, tokensIn: s2.tokensIn, tokensOut: s2.tokensOut,
                        cacheReadTokens: s2.cacheReadTokens, observedAt: now, workingDirectory: home + "/work/hud-ios"),
            LiveSession(id: "s3", agentId: "claude-sonnet", task: "optimize db queries", terminal: "etl",
                        startedAt: now.addingTimeInterval(-140 * 60), endedAt: now.addingTimeInterval(-51 * 60),
                        pctOfWindow: 2.1, tokensIn: s3.tokensIn, tokensOut: s3.tokensOut, cacheReadTokens: s3.cacheReadTokens,
                        workingDirectory: home + "/data/etl"),
            LiveSession(id: "s4", agentId: "chatgpt", task: L10n.text("桌面版 · 3 段对话", "Desktop · 3 conversations"), terminal: nil,
                        startedAt: now.addingTimeInterval(-200 * 60), endedAt: now.addingTimeInterval(-120 * 60),
                        pctOfWindow: 4.5, tokensIn: 0, tokensOut: 0, observedAt: now),
        ]
    }

    private typealias Plan = (id: String, agent: String, helper: String?, priced: String, minutes: (Double, Double), turns: Int, seed: Int)

    /// Each demo session's model, the other Claude model a Claude session hands work to, the model that prices its calls,
    /// when it ran in minutes from now, its turns and a seed.
    private static let plans: [Plan] = [
        ("s1", "claude-opus", "claude-sonnet", "claude-model:claude-opus-5", (-27, 0), 26, 7),
        ("s2", "codex", nil, "codex-model:gpt-5.6-sol", (-64, 0), 17, 19),
        ("s3", "claude-sonnet", "claude-opus", "claude-model:claude-sonnet-5", (-140, -51), 30, 51),
    ]

    /// The demo sessions' turns, each the sum of its calls: now and then a large paste, cache writes that carry the
    /// previous turn forward, reasoning on about half of them, and a compaction once the context nears its window. Claude
    /// sessions hand every fourth turn's work partly to a sub-agent on the other Claude model, and two thirds of the way in
    /// come back after their cache lapsed, writing the whole context again.
    public static func sessionUsage(now: Date) -> [String: SessionUsage] {
        let window = 200_000
        var result: [String: SessionUsage] = [:]
        for plan in plans {
            var state = UInt64(plan.seed * 9973 + 1)
            func random() -> Double {
                state = state * 16807 % 2_147_483_647
                return Double(state) / 2_147_483_647
            }
            let start = now.addingTimeInterval(plan.minutes.0 * 60), span = (plan.minutes.1 - plan.minutes.0) * 60
            var context = 0, carry = 12_000, calls = 0, cost: Decimal = 0
            var turns: [SessionUsage.Turn] = [], models: [String: SessionUsage.Tokens] = [:], periods: [Date: [String: SessionUsage.Tokens]] = [:]
            var helperTokens: SessionUsage.Tokens?
            for index in 0..<plan.turns {
                let input = Int(random() < 0.18 ? 7_000 + random() * 16_000 : 150 + random() * 1_800)
                let output = Int(250 + random() * random() * 4_200)
                let reasoning = random() < 0.55 ? Int(400 + random() * 5_000) : 0
                var write = carry, read = context, compacted = false, peak: Int?
                if context + write + input > window * 82 / 100 {
                    compacted = true; peak = context + write + input; read = 0; write = 26_000
                }
                let lapsed = plan.helper != nil && index == plan.turns * 2 / 3 && !compacted
                if lapsed { write += read; read = 0 }
                let added = SessionUsage.Tokens(tokensIn: input + write, tokensOut: output + reasoning, cacheReadTokens: 0,
                                                cacheWriteTokens: write, reasoningTokens: reasoning)
                let turnStart = start.addingTimeInterval(span * (Double(index) + random() * 0.4) / Double(plan.turns))
                let turnEnd = min(now, turnStart.addingTimeInterval(20 + random() * 150))
                let helping = plan.helper != nil && index % 4 == 1 && !lapsed
                let outline = SessionUsage.Turn(start: turnStart, end: turnEnd, tokens: added, calls: 3 + Int(random() * 12),
                                                contextTokens: read + write + input, compacted: compacted,
                                                subagents: helping ? Self.part(of: added) : nil, peakContextTokens: peak,
                                                recachedTokens: lapsed ? input + write : nil)
                let turnCalls = Self.calls(of: outline, plan: plan)
                let own = turnCalls.filter(\.own).reduce(SessionUsage.Tokens()) { $0 + $1.tokens }
                let helper = turnCalls.filter { !$0.own }.reduce(SessionUsage.Tokens()) { $0 + $1.tokens }
                let turnCost = turnCalls.reduce(Decimal(0)) { $0 + ($1.listCost ?? 0) }
                let recached = turnCalls.compactMap(\.recached).reduce(0, +)
                turns.append(.init(start: turnStart, end: turnEnd, tokens: own + helper, calls: turnCalls.count,
                                   contextTokens: outline.contextTokens, compacted: compacted, subagents: helping ? helper : nil,
                                   peakContextTokens: peak, recachedTokens: recached > 0 ? recached : nil, listCost: turnCost))
                calls += turnCalls.count
                cost += turnCost
                let period = Date(timeIntervalSince1970: (turnStart.timeIntervalSince1970 / UsageBucket.duration).rounded(.down) * UsageBucket.duration)
                models[plan.agent, default: .init()] += own
                periods[period, default: [:]][plan.agent, default: .init()] += own
                if let name = plan.helper, helping {
                    models[name, default: .init()] += helper
                    periods[period, default: [:]][name, default: .init()] += helper
                    helperTokens = (helperTokens ?? .init()) + helper
                }
                context = read + write
                carry = input + output
            }
            result[plan.id] = SessionUsage(
                models: models.map { SessionUsage.Model(agentId: $0.key, tokens: $0.value) }.sorted { $0.tokens.tokensIn > $1.tokens.tokensIn },
                periods: periods.keys.sorted().flatMap { start in periods[start]!.keys.sorted().map { SessionUsage.Period(start: start, agentId: $0, tokens: periods[start]![$0]!) } },
                subagents: helperTokens, calls: calls, contextTokens: turns.last?.contextTokens, turns: turns, contextWindow: window,
                listCost: cost)
        }
        return result
    }

    /// A demo turn's calls, as the session page lays them out.
    public static func turnCalls(session: String, turn: SessionUsage.Turn) -> [TurnCall] {
        plans.first { $0.id == session }.map { calls(of: turn, plan: $0) } ?? []
    }

    /// A turn's calls, decided by what the turn added rather than by its cache reads or price, which are the calls' sums.
    /// The session's own calls read a context that grows to where the turn ended: a re-cached turn first writes it all
    /// again, a compacted one first reads its peak. One call reads in a large tool result; a sub-agent's calls follow the
    /// call that handed it work.
    private static func calls(of turn: SessionUsage.Turn, plan: Plan) -> [TurnCall] {
        var state = UInt64(RecordCoding.milliseconds(turn.start) % 2_147_483_646 + 1)
        func random() -> Double {
            state = state * 16807 % 2_147_483_647
            return Double(state) / 2_147_483_647
        }
        func split(_ total: Int, _ weights: [Double]) -> [Int] {
            let sum = weights.reduce(0, +)
            var parts = weights.map { Int(Double(max(0, total)) * $0 / sum) }
            if !parts.isEmpty { parts[parts.count - 1] += max(0, total) - parts.reduce(0, +) }
            return parts
        }
        let sub = turn.subagents ?? .init(), all = turn.tokens
        let subCount = turn.subagents == nil ? 0 : max(1, turn.calls / 3), ownCount = max(2, turn.calls - subCount)
        let ownWrite = all.cacheWriteTokens - sub.cacheWriteTokens
        let ownFresh = all.tokensIn - all.cacheWriteTokens - (sub.tokensIn - sub.cacheWriteTokens)
        let burst = 1 + Int(random() * Double(ownCount - 1)), handoff = max(0, min(ownCount - 2, burst))
        var inWeights = (0..<ownCount).map { _ in 0.3 + random() }
        inWeights[burst] += 4
        if turn.recachedTokens != nil { inWeights[0] += 1_000 }
        let outWeights = (0..<ownCount).map { _ in 0.2 + random() }
        var writes = split(ownWrite, inWeights)
        if turn.compacted { writes = [0] + split(ownWrite, Array(inWeights.dropFirst())) }
        let fresh = split(ownFresh, inWeights)
        let thoughts = split(all.reasoningTokens - sub.reasoningTokens, outWeights)
        let outputs = split(all.tokensOut - all.reasoningTokens - (sub.tokensOut - sub.reasoningTokens), outWeights)
        let tools = [["Grep"], ["Edit"], ["Bash"], ["Read"], ["Edit"]]
        let source = plan.agent == "codex" ? "codex" : "claude"
        let times = { (position: Int, count: Int) -> Date in
            turn.start.addingTimeInterval(2 + max(0, turn.end.timeIntervalSince(turn.start) - 2) * Double(position) / Double(max(1, count - 1)))
        }
        func call(_ agentId: String, own: Bool, _ tokens: SessionUsage.Tokens, context: Int?, recached: Int?, tools: [String]) -> TurnCall {
            TurnCall(timestamp: .distantPast, agentId: agentId, tokens: tokens, own: own,
                     log: own ? "demo/\(plan.id).jsonl" : "demo/\(plan.id)/subagent.jsonl", source: source, context: context,
                     recached: recached, listCost: ModelCatalog.cost(agentId: plan.priced, kinds: tokens.kinds)?.amount, tools: tools)
        }
        var result: [TurnCall] = [], prompt = 0
        for index in 0..<ownCount {
            let read: Int
            if index == 0, turn.recachedTokens != nil { read = 0 }
            else if index == 0, turn.compacted, let peak = turn.peakContextTokens { read = max(0, peak - fresh[0]) }
            else if index == 0 { read = max(0, (turn.contextTokens ?? 0) - ownWrite - ownFresh) }
            else if index == 1, turn.compacted { read = 0 }
            else { read = prompt }
            let tokens = SessionUsage.Tokens(tokensIn: writes[index] + fresh[index], tokensOut: outputs[index] + thoughts[index],
                                             cacheReadTokens: read, cacheWriteTokens: writes[index], reasoningTokens: thoughts[index])
            prompt = read + writes[index] + fresh[index]
            let asked = index == ownCount - 1 ? [] : index == handoff && subCount > 0 ? ["Task"] : index == burst - 1 ? ["Read"]
                : index == 0 ? ["Read", "Read"] : tools[index % tools.count]
            result.append(call(plan.agent, own: true, tokens, context: prompt,
                               recached: index == 0 && turn.recachedTokens != nil ? tokens.tokensIn : nil, tools: asked))
            guard index == handoff, subCount > 0 else { continue }
            let subWrites = split(sub.cacheWriteTokens, Array(repeating: 1, count: subCount))
            let subFresh = split(sub.tokensIn - sub.cacheWriteTokens, Array(repeating: 1, count: subCount))
            let subThoughts = split(sub.reasoningTokens, Array(repeating: 1, count: subCount))
            let subOutputs = split(sub.tokensOut - sub.reasoningTokens, Array(repeating: 1, count: subCount))
            var subPrompt = 0
            for part in 0..<subCount {
                let tokens = SessionUsage.Tokens(tokensIn: subWrites[part] + subFresh[part], tokensOut: subOutputs[part] + subThoughts[part],
                                                 cacheReadTokens: subPrompt, cacheWriteTokens: subWrites[part], reasoningTokens: subThoughts[part])
                subPrompt += subWrites[part] + subFresh[part] + 9_000
                result.append(call(plan.helper ?? plan.agent, own: false, tokens, context: nil, recached: nil,
                                   tools: part == subCount - 1 ? [] : [["Read"], ["Grep"], ["Glob"]][part % 3]))
            }
        }
        return result.enumerated().map { position, call in
            TurnCall(timestamp: times(position, result.count), agentId: call.agentId, tokens: call.tokens, own: call.own, log: call.log,
                     source: call.source, context: call.context, recached: call.recached, listCost: call.listCost, tools: call.tools)
        }
    }

    /// The share of a turn its sub-agent spent.
    private static func part(of tokens: SessionUsage.Tokens) -> SessionUsage.Tokens {
        SessionUsage.Tokens(tokensIn: tokens.tokensIn * 2 / 5, tokensOut: tokens.tokensOut * 2 / 5, cacheReadTokens: tokens.cacheReadTokens * 2 / 5,
                            cacheWriteTokens: tokens.cacheWriteTokens * 2 / 5, reasoningTokens: tokens.reasoningTokens * 2 / 5)
    }

    public static func insights(now: Date, calendar: Calendar = .current) -> UsageInsights {
        // "周二 16:10" of the current week.
        var components = calendar.dateComponents([.yearForWeekOfYear, .weekOfYear], from: now)
        components.weekday = 3
        components.hour = 16
        components.minute = 10
        let tuesday = calendar.date(from: components)
        return UsageInsights(
            burnRatePctPerHour: 8.4,
            timeToExhaust: 2 * 3600 + 40 * 60,
            weeklyCapHits: 3,
            weeklyWaitTotal: 1 * 3600 + 52 * 60,
            weeklyWaitLongest: 58 * 60,
            weeklyWaitLongestAt: tuesday
        )
    }
}
