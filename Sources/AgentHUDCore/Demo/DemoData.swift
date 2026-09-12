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

    /// Remaining % and seconds until reset per agent.
    public static let quota: [String: (remaining: Double, resetIn: TimeInterval?, weekly: Double?)] = [
        "claude-opus": (72, 2 * 3600 + 14 * 60, 61),
        "claude-sonnet": (24, 2 * 3600 + 14 * 60, 61),
        "chatgpt": (58, 4 * 3600 + 2 * 60, 80),
        "codex": (7, 51 * 60, nil),
        "antigravity": (91, 6 * 3600 + 40 * 60, nil),
        "deepseek": (44, nil, nil),
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
            costs: [.init(timestamp: now.addingTimeInterval(-60), sessionId: "deepseek-sample", model: "deepseek-v4-flash",
                          amounts: ["CNY": Decimal(string: "0.01239")!])],
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
            weeklyWaitLongestAt: tuesday,
            weeklyShare: ["claude-opus": 0.46, "claude-sonnet": 0.24, "chatgpt": 0.18, "codex": 0.12],
            windowSessionCount: 6,
            windowUsedPct: 28
        )
    }
}
