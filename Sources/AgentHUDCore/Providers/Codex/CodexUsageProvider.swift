import AgentHUDSupport
import Foundation

public actor CodexUsageProvider: UsageProvider, LedgerRecording {
    private let readLimits: @Sendable () async throws -> CodexRateLimits
    private let transcripts: CodexTranscriptStore
    private let history: QuotaHistoryStore
    private let clock: @Sendable () -> Date
    /// `ClientHome.key` of the Codex home this provider reads.
    private let home: String
    private let readPiLimits: @Sendable () async throws -> CodexRateLimits?
    private let piHome: String
    private var lastRequestAt: Date?
    private struct Reading {
        let limits: CodexRateLimits
        let at: Date
    }
    private var readings: [String: Reading] = [:]
    private var failures: [String: String] = [:]
    /// The email each home's workspace last came with, by home and workspace hash, kept across launches: the email is
    /// part of the account's key, and `account/read` can answer too late for a reading to carry it.
    private var emails: [String: String] = [:]
    private let identityCacheURL: URL?

    public init(readLimits: @escaping @Sendable () async throws -> CodexRateLimits,
                transcripts: CodexTranscriptStore, history: QuotaHistoryStore, home: String = "",
                clock: @escaping @Sendable () -> Date = { Date() },
                readPiLimits: @escaping @Sendable () async throws -> CodexRateLimits? = { nil }, piHome: String = "pi",
                identityCacheURL: URL? = nil) {
        self.readLimits = readLimits; self.transcripts = transcripts; self.history = history; self.home = home; self.clock = clock
        self.readPiLimits = readPiLimits; self.piHome = piHome; self.identityCacheURL = identityCacheURL
        if let data = identityCacheURL.flatMap({ try? Data(contentsOf: $0) }),
           let saved = try? JSONDecoder().decode([String: String].self, from: data) {
            emails = saved
        }
    }

    public static func standard(ledger: UsageLedger) -> CodexUsageProvider {
        let directory = CodexLocator.dataDirectory
        let pi = PiCodexClient(directory: PiCodexClient.directory)
        return CodexUsageProvider(readLimits: {
            guard let executable = CodexLocator.find() else {
                throw UsageProviderError(L10n.text("安装并登录后即可读取额度", "Install and sign in to read quota"))
            }
            return try await CodexAppServerClient(executable: executable, dataDirectory: directory).fetch()
        }, transcripts: .standard(directory: directory, ledger: ledger),
           history: QuotaHistoryStore(ledger: ledger, scope: "codex", importing: AppSupport.directory.appendingPathComponent("codex-quota-history.json")),
           home: ClientHome.key(directory, defaultDirectory: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex", isDirectory: true)),
           readPiLimits: { try await pi.fetch() },
           piHome: "pi:" + ClientHome.key(pi.directory, defaultDirectory: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".pi/agent")),
           identityCacheURL: AppSupport.directory.appendingPathComponent("codex-identities.json"))
    }

    public nonisolated var watchedDirectories: [URL]? { transcripts.roots }
    // Pi and other clients can spend the same account without writing Codex rollouts.
    public nonisolated var seesLocalWork: Bool { false }

    public func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
        guard lastRequestAt.map({ now.timeIntervalSince($0) >= UsageRefresh.accountRequestSpacing }) ?? true else { return }
        lastRequestAt = now
        await read(home: home) { try await self.readLimits() }
        await read(home: piHome, fetch: readPiLimits)
        // Several clients can read the same account. Its history, windows and alerts have one owner.
        for (source, reading) in accountReadings where failures[source] == nil {
            await history.append(reading.limits.rows(home: source).map {
                QuotaSample(agentId: $0.id, timestamp: reading.at, remainingPct: $0.window.remainingPct)
            }, now: now)
        }
    }

    private func read(home: String, fetch: @Sendable () async throws -> CodexRateLimits?) async {
        do {
            if let limits = try await fetch() { readings[home] = Reading(limits: identified(limits, home: home), at: clock()) }
            else { readings[home] = nil }
            failures[home] = nil
        } catch {
            guard !Task.isCancelled else { return }
            let message = error.localizedDescription
            failures[home] = message
        }
    }

    /// A reading whose `account/read` did not answer takes the email its workspace last came with on this home, so the
    /// account keeps its key instead of reappearing under a second one without the email.
    private func identified(_ limits: CodexRateLimits, home: String) -> CodexRateLimits {
        guard let workspace = limits.accountId?.trimmingCharacters(in: .whitespacesAndNewlines), !workspace.isEmpty else { return limits }
        let key = home + "/" + RecordCoding.hash([workspace])
        if let email = limits.account?.email, !email.isEmpty {
            if emails[key] != email {
                emails[key] = email
                saveEmails()
            }
            return limits
        }
        guard limits.account == nil, let email = emails[key] else { return limits }
        var filled = limits
        filled.account = .init(type: "chatgpt", email: email, planType: nil)
        return filled
    }

    private func saveEmails() {
        guard let identityCacheURL else { return }
        do {
            try FileManager.default.createDirectory(at: identityCacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try JSONEncoder().encode(emails).write(to: identityCacheURL, options: .atomic)
        } catch { NSLog("[AgentHUD] Codex identity cache write failed: %@", error.localizedDescription) }
    }

    private var accountReadings: [(String, Reading)] {
        var accounts: [String: (String, Reading)] = [:]
        for source in [home, piHome] {
            guard let reading = readings[source] else { continue }
            let key = reading.limits.providerAccount(home: source).id
            // Prefer a successful native read, then a successful Pi read over a failed native read.
            if let old = accounts[key], failures[old.0] == nil || failures[source] != nil { continue }
            accounts[key] = (source, reading)
        }
        return accounts.values.sorted { $0.1.limits.providerAccount(home: $0.0).id < $1.1.limits.providerAccount(home: $1.0).id }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock()
        let weekAgo = now.addingTimeInterval(-7 * 86400)
        let indexed = await transcripts.index(since: min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600)))
        let selected = accountReadings
        let native = readings[home]
        let windows = selected.flatMap { source, reading in
            reading.limits.rows(home: source).map { (row: $0, reading: reading) }
        }
        let models = Set(indexed.sessions.flatMap(\.transcript.models)).sorted()
        let consumers = models.map { AgentDescriptor(id: "codex-model:\($0)", vendor: "Codex", model: $0,
                                                     source: L10n.sourceCodexAppServer, enabled: true) }
        let snapshots = windows.map { row, reading in
            UsageSnapshot(agentId: row.id, remainingPct: row.window.remainingPct, weeklyRemainingPct: row.weekly?.remainingPct,
                          resetAt: row.window.resetAt, windowDuration: row.window.duration,
                          weeklyResetAt: row.weekly?.resetAt, updatedAt: reading.at)
        }
        var byAgent: [String: UsageInsights] = [:]
        for (entry, snapshot) in zip(windows, snapshots) {
            let row = entry.row
            let samples = await history.samples(agentId: row.id, since: min(weekAgo, snapshot.cycle?.start ?? weekAgo))
            let burn = UsageAnalytics.burnRate(samples: samples, cycle: snapshot.cycle, now: now)
            let caps = UsageAnalytics.capStats(samples: samples.filter { $0.timestamp >= weekAgo }, now: now)
            byAgent[row.id] = UsageInsights(burnRatePctPerHour: burn?.pctPerHour,
                                            timeToExhaust: burn?.timeToExhaust(remainingPct: row.window.remainingPct),
                                            weeklyCapHits: caps.hits, weeklyWaitTotal: caps.totalWait,
                                            weeklyWaitLongest: caps.longestWait, weeklyWaitLongestAt: caps.longestAt)
        }
        let sessions = indexed.sessions.filter { !$0.transcript.isSubagent }.sorted { a, b in
            let al = a.transcript.isLive(now: now, modifiedAt: a.modifiedAt), bl = b.transcript.isLive(now: now, modifiedAt: b.modifiedAt)
            if al != bl { return al }
            return (a.transcript.lastActivityAt ?? .distantPast) > (b.transcript.lastActivityAt ?? .distantPast)
        }.map { session in
            let t = session.transcript
            return LiveSession(id: t.id!, agentId: "codex-model:\(t.model)",
                               task: session.title ?? t.task ?? t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Codex",
                               terminal: t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: t.startedAt ?? session.modifiedAt,
                               endedAt: t.isLive(now: now, modifiedAt: session.modifiedAt) ? nil : (t.lastActivityAt ?? session.modifiedAt),
                               pctOfWindow: nil, tokensIn: t.inputTokens,
                               tokensOut: t.outputTokens, client: t.client, transcriptPath: session.path,
                               cacheReadTokens: t.cachedInputTokens, observedAt: now)
        }
        var notices = Dictionary(uniqueKeysWithValues: selected.compactMap { source, reading -> (String, String)? in
            failures[source].map { (reading.limits.providerAccount(home: source).id, $0) }
        })
        for (source, message) in failures where readings[source] == nil {
            notices[source == piHome ? "Pi" : "Codex login"] = message
        }
        let notice = notices.isEmpty ? nil : notices.values.sorted().joined(separator: " · ")
        let consumerIds = Set(consumers.map(\.id) + sessions.map(\.agentId))
        // Pi's distinct account must not claim Codex transcript consumers. Pi owns its own token events.
        let nativeAccount = native?.limits.providerAccount(home: home).id
        var consumerIdsByQuota = Dictionary(uniqueKeysWithValues: windows.map {
            ($0.row.id, $0.row.account?.id == nativeAccount ? consumerIds : Set<String>())
        })
        for agent in agents where agent.vendor == "Codex" && agent.account == nil {
            consumerIdsByQuota[agent.id] = consumerIds
        }
        let observations = selected.map { source, reading in
            AccountObservation(account: reading.limits.providerAccount(home: source), home: source,
                label: reading.limits.account?.email, plan: reading.limits.plan, observedAt: reading.at,
                quotaNotice: failures[source], resetCredits: reading.limits.rateLimitResetCredits,
                aliases: reading.limits.keyWithoutEmail.map { [$0] })
        }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: sessions,
                           notice: notice, discoveredAgents: windows.map { $0.row.descriptor }, consumers: consumers,
                           indexing: indexed.indexing, insightsByAgent: byAgent,
                           subscriptions: native?.limits.plan.map { ["Codex": $0] } ?? [:],
                           sourceNotices: selected.isEmpty ? notice.map { ["Codex": $0] } ?? [:] : notices,
                           consumerIdsByQuota: consumerIdsByQuota, codexResetCredits: selected.count == 1 ? selected.first?.1.limits.rateLimitResetCredits : nil,
                           codexResetCreditsObservedAt: selected.count == 1 && selected.first?.1.limits.rateLimitResetCredits != nil ? selected.first?.1.at : nil,
                           completions: indexed.sessions.flatMap { $0.transcript.completions ?? [] },
                           turns: indexed.sessions.flatMap { $0.transcript.sessionTurns },
                           accounts: lastRequestAt == nil || selected.isEmpty && !failures.isEmpty ? nil : ["Codex": observations])
    }
}
