import Foundation

/// The agents, sessions and figures shown in the design handoff.
public enum DemoData {
    public static let agents: [AgentDescriptor] = [
        AgentDescriptor(id: "claude-opus", vendor: "Claude", model: "Opus 4.5", source: L10n.sourceClaudeSessions, enabled: true),
        AgentDescriptor(id: "claude-sonnet", vendor: "Claude", model: "Sonnet 4.5", source: L10n.sourceClaudeSessions, enabled: true),
        AgentDescriptor(id: "chatgpt", vendor: "ChatGPT", model: "GPT‑5 Plus", source: L10n.sourceBrowserAuth, enabled: true),
        AgentDescriptor(id: "codex", vendor: "Codex", model: "CLI", source: L10n.sourceCodexAppServer, enabled: true),
        AgentDescriptor(id: "antigravity", vendor: "Antigravity", model: "Agent", source: L10n.sourceNotConnected, enabled: false, connected: false),
        AgentDescriptor(id: "deepseek", vendor: "DeepSeek", model: "Harness", source: L10n.sourceNotConnected, enabled: false, connected: false),
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
        [
            LiveSession(id: "s1", agentId: "claude-opus", task: "fix auth bug in middleware", terminal: "iTerm",
                        startedAt: now.addingTimeInterval(-27 * 60), pctOfWindow: 6.2, tokensIn: 48_000, tokensOut: 12_000, cacheReadTokens: 96_000, observedAt: now),
            LiveSession(id: "s2", agentId: "codex", task: "backend server endpoints", terminal: "Terminal",
                        startedAt: now.addingTimeInterval(-64 * 60), pctOfWindow: 3.8, tokensIn: 31_000, tokensOut: 9_000, cacheReadTokens: 62_000, observedAt: now),
            LiveSession(id: "s3", agentId: "claude-sonnet", task: "optimize db queries", terminal: "Ghostty",
                        startedAt: now.addingTimeInterval(-140 * 60), endedAt: now.addingTimeInterval(-51 * 60),
                        pctOfWindow: 2.1, tokensIn: 19_000, tokensOut: 4_000, cacheReadTokens: 38_000),
            LiveSession(id: "s4", agentId: "chatgpt", task: L10n.text("桌面版 · 3 段对话", "Desktop · 3 conversations"), terminal: nil,
                        startedAt: now.addingTimeInterval(-200 * 60), endedAt: now.addingTimeInterval(-120 * 60),
                        pctOfWindow: 4.5, tokensIn: 0, tokensOut: 0, observedAt: now),
        ]
    }

    /// Each demo session's tokens spread over its 15-minute periods, heavier toward the end, with a sub-agent on Claude.
    public static func sessionUsage(now: Date) -> [String: SessionUsage] {
        var result: [String: SessionUsage] = [:]
        for session in sessions(now: now) where session.hasTokenCounts {
            let end = session.endedAt ?? now, step = UsageBucket.duration
            let first = (session.startedAt.timeIntervalSinceReferenceDate / step).rounded(.down) * step
            let starts = stride(from: first, through: end.timeIntervalSinceReferenceDate, by: step).map { Date(timeIntervalSinceReferenceDate: $0) }
            let weights = starts.indices.map { Double($0 + 2) }, sum = weights.reduce(0, +)
            let share = { (value: Int, index: Int) in Int(Double(value) * weights[index] / sum) }
            // A Claude session hands every third period to a sub-agent on the other Claude model.
            let helper = session.agentId.hasPrefix("claude") ? (session.agentId == "claude-sonnet" ? "claude-opus" : "claude-sonnet") : nil
            let periods = starts.indices.map { index in
                SessionUsage.Period(start: starts[index], agentId: helper != nil && index % 3 == 1 ? helper! : session.agentId,
                                    tokens: .init(tokensIn: share(session.tokensIn, index), tokensOut: share(session.tokensOut, index),
                                                  cacheReadTokens: share(session.cacheReadTokens, index)))
            }
            let models = Dictionary(grouping: periods, by: \.agentId).map { agentId, periods in
                SessionUsage.Model(agentId: agentId, tokens: periods.reduce(SessionUsage.Tokens()) { $0 + $1.tokens })
            }.sorted { $0.tokens.tokensIn + $0.tokens.tokensOut > $1.tokens.tokensIn + $1.tokens.tokensOut }
            result[session.id] = SessionUsage(models: models, periods: periods, subagents: models.first { $0.agentId == helper }?.tokens,
                                              calls: periods.count * 9, contextTokens: session.cacheReadTokens / 3 + 12_000)
        }
        return result
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
