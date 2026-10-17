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
    private let readLoginAt: @Sendable (String) -> Date?
    private var lastRequestAt: Date?
    private struct Reading {
        let limits: CodexRateLimits
        let at: Date
    }
    private var readings: [String: Reading] = [:]
    private var failures: [String: String] = [:]
    /// The email each home's workspace last came with, by home and workspace hash, kept across launches. Both are part of
    /// the account's key, and a reading can lack either: `account/read` can answer too late for the email, and an engine
    /// can leave out `accountId`.
    private var emails: [String: String] = [:]
    private struct ClientLogin: Codable, Equatable {
        let home: String
        let client: String
        let loggedInAt: Date?
        let confirmedAt: Date
    }
    private struct IdentityCache: Codable {
        let emails: [String: String]
        let clients: [String: ClientLogin]
    }
    private var clients: [String: ClientLogin] = [:]
    private let identityCacheURL: URL?
    private let sessionOriginsDirectory: URL?
    private let readTerminalOrigins: @Sendable (Set<String>) async -> [String: SessionNavigationTarget]

    public init(readLimits: @escaping @Sendable () async throws -> CodexRateLimits,
                transcripts: CodexTranscriptStore, history: QuotaHistoryStore, home: String = "",
                clock: @escaping @Sendable () -> Date = { Date() },
                readPiLimits: @escaping @Sendable () async throws -> CodexRateLimits? = { nil }, piHome: String = "pi",
                identityCacheURL: URL? = nil, readLoginAt: @escaping @Sendable (String) -> Date? = { _ in nil },
                initialAccounts: [AccountObservation] = [], sessionOriginsDirectory: URL? = nil,
                readTerminalOrigins: @escaping @Sendable (Set<String>) async -> [String: SessionNavigationTarget] = { _ in [:] }) {
        self.readLimits = readLimits; self.transcripts = transcripts; self.history = history; self.home = home; self.clock = clock
        self.readPiLimits = readPiLimits; self.piHome = piHome; self.identityCacheURL = identityCacheURL
        self.readLoginAt = readLoginAt
        self.sessionOriginsDirectory = sessionOriginsDirectory
        self.readTerminalOrigins = readTerminalOrigins
        if let data = identityCacheURL.flatMap({ try? Data(contentsOf: $0) }) {
            if let saved = try? JSONDecoder().decode(IdentityCache.self, from: data) {
                emails = saved.emails; clients = saved.clients
            } else if let saved = try? JSONDecoder().decode([String: String].self, from: data) {
                emails = saved
            }
        }
        // Earlier reports already chose a client. Keep that choice when its client has no reliable sign-in time.
        for account in initialAccounts.sorted(by: { ($0.isCurrent ? 1 : 0, $0.observedAt) > ($1.isCurrent ? 1 : 0, $1.observedAt) })
            where clients[account.account.id] == nil {
            clients[account.account.id] = ClientLogin(home: account.home, client: account.client,
                loggedInAt: nil, confirmedAt: account.observedAt)
        }
    }

    /// `persistent` false imports no earlier version's quota history and remembers no workspace's email.
    public static func standard(ledger: UsageLedger, persistent: Bool = true) -> CodexUsageProvider {
        let directory = CodexLocator.dataDirectory
        let pi = PiCodexClient(directory: PiCodexClient.directory)
        let nativeHome = ClientHome.key(directory, defaultDirectory: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex", isDirectory: true))
        let piHome = "pi:" + ClientHome.key(pi.directory, defaultDirectory: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".pi/agent"))
        let cached = persistent ? (try? Data(contentsOf: AppSupport.directory.appendingPathComponent("last-usage-report.json"))) : nil
        let initial = cached.flatMap { try? JSONDecoder().decode(UsageReport.self, from: $0) }?.accounts?["Codex"] ?? []
        try? CodexSessionOrigins.prepare()
        return CodexUsageProvider(readLimits: {
            guard let executable = CodexLocator.find() else {
                throw UsageProviderError(L10n.text("安装并登录后即可读取额度", "Install and sign in to read quota"))
            }
            return try await CodexAppServerClient(executable: executable, dataDirectory: directory).fetch()
        }, transcripts: .standard(directory: directory, ledger: ledger),
           history: QuotaHistoryStore(ledger: ledger, scope: "codex",
                                      importing: persistent ? AppSupport.directory.appendingPathComponent("codex-quota-history.json") : nil),
           home: nativeHome,
           readPiLimits: { try await pi.fetch() },
           piHome: piHome,
           identityCacheURL: persistent ? AppSupport.directory.appendingPathComponent("codex-identities.json") : nil,
           readLoginAt: { $0 == piHome ? CodexLoginTime.pi(in: pi.directory) : CodexLoginTime.native(in: directory) },
           initialAccounts: initial, sessionOriginsDirectory: CodexSessionOrigins.directory,
           readTerminalOrigins: { await CodexTerminalOrigins.read(threadIDs: $0, dataDirectory: directory) })
    }

    public nonisolated var watchedDirectories: [URL]? {
        transcripts.roots + (sessionOriginsDirectory.map { [$0] } ?? [])
    }
    public func fileChanges(_ paths: Set<String>?) async { await transcripts.fileChanges(paths) }
    // Pi and other clients can spend the same account without writing Codex rollouts.
    public nonisolated var seesLocalWork: Bool { false }

    public func refreshAccountUsage(historyHours: Int) async {
        let now = clock()
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
            if let limits = try await fetch() {
                let limits = identified(limits, home: home)
                readings[home] = Reading(limits: limits, at: clock())
                let account = limits.providerAccount(home: home).id
                forgetOtherClients(home: home, keeping: account)
                rememberClient(account: account, home: home)
            }
            else { readings[home] = nil; forgetOtherClients(home: home, keeping: nil) }
            failures[home] = nil
        } catch {
            guard !Task.isCancelled else { return }
            let message = error.localizedDescription
            failures[home] = message
        }
    }

    /// A reading with half of its account's key takes the other half from the last reading on this home that had both,
    /// so the account keeps its key instead of reappearing under a second one: a workspace whose `account/read` did not
    /// answer takes the email it came with, and an email without `accountId` the workspace it came with, unless it came
    /// with several.
    private func identified(_ limits: CodexRateLimits, home: String) -> CodexRateLimits {
        let workspace = limits.accountId?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let email = limits.account?.email ?? ""
        var filled = limits
        if !workspace.isEmpty, !email.isEmpty {
            let key = home + "/" + RecordCoding.hash([workspace])
            if emails[key] != email {
                emails[key] = email
                saveIdentities()
            }
        } else if !workspace.isEmpty {
            if limits.account == nil, let known = emails[home + "/" + RecordCoding.hash([workspace])] {
                filled.account = .init(type: "chatgpt", email: known, planType: nil)
            }
        } else if !email.isEmpty {
            let workspaces = emails.compactMap { key, known -> String? in
                guard known.lowercased() == email.lowercased(), let slash = key.lastIndex(of: "/"), key[..<slash] == home else { return nil }
                return String(key[key.index(after: slash)...])
            }
            if workspaces.count == 1 { filled.rememberedWorkspace = workspaces[0] }
        }
        return filled
    }

    private func rememberClient(account: String, home: String) {
        let login = readLoginAt(home)
        // A cached owner outside the configured homes cannot read again after the user changes a client directory.
        if let previous = clients[account], previous.home == self.home || previous.home == piHome {
            if previous.home == home, previous.loggedInAt == nil, let login {
                clients[account] = ClientLogin(home: home, client: previous.client, loggedInAt: login, confirmedAt: previous.confirmedAt)
            } else {
                guard let login, login > (previous.loggedInAt ?? previous.confirmedAt) else { return }
                clients[account] = ClientLogin(home: home, client: home == piHome ? "Pi" : "Codex", loggedInAt: login, confirmedAt: clock())
            }
        } else {
            clients[account] = ClientLogin(home: home, client: home == piHome ? "Pi" : "Codex", loggedInAt: login, confirmedAt: clock())
        }
        saveIdentities()
    }

    private func forgetOtherClients(home: String, keeping account: String?) {
        let remaining = clients.filter { $0.value.home != home || $0.key == account }
        guard remaining.count != clients.count else { return }
        clients = remaining
        saveIdentities()
    }

    private func saveIdentities() {
        guard let identityCacheURL else { return }
        do {
            try FileManager.default.createDirectory(at: identityCacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try JSONEncoder().encode(IdentityCache(emails: emails, clients: clients)).write(to: identityCacheURL, options: .atomic)
        } catch { NSLog("[AgentHUD] Codex identity cache write failed: %@", error.localizedDescription) }
    }

    private var accountReadings: [(String, Reading)] {
        var accounts: [String: (String, Reading)] = [:]
        for source in [home, piHome] {
            guard let reading = readings[source] else { continue }
            let key = reading.limits.providerAccount(home: source).id
            guard accounts[key] == nil else { continue }
            let owner = clients[key]?.home ?? source
            guard let owned = readings[owner], owned.limits.providerAccount(home: owner).id == key else { continue }
            accounts[key] = (owner, owned)
        }
        return accounts.values.sorted { $0.1.limits.providerAccount(home: $0.0).id < $1.1.limits.providerAccount(home: $1.0).id }
    }

    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport {
        let now = clock()
        let weekAgo = now.addingTimeInterval(-AlertPolicy.insightsLookback)
        let indexed = await transcripts.index(since: min(weekAgo, now.addingTimeInterval(-Double(historyHours) * 3600)))
        // Tokens and models come from every rollout of `historyHours`; sessions only from those active in the session
        // window that Codex did not archive. Spawned agents and guardians keep rollouts of their own that name the thread
        // that started them; a session reported takes every rollout below it, whenever that was last written, and so
        // does its breakdown.
        let sessionStart = SessionWindow.start(at: now)
        var children: [String: [CodexTranscriptStore.Session]] = [:]
        for session in indexed.sessions where session.transcript.isSubagent {
            if let parent = session.transcript.parentThreadID { children[parent, default: []].append(session) }
        }
        func descendants(of id: String) -> [CodexTranscriptStore.Session] {
            var found: [CodexTranscriptStore.Session] = [], queue = [id], seen: Set<String> = [id]
            while let next = queue.popLast() {
                for child in children[next] ?? [] {
                    found.append(child)
                    if let childID = child.transcript.id, seen.insert(childID).inserted { queue.append(childID) }
                }
            }
            return found
        }
        let recent = indexed.sessions.filter { !$0.archived && ($0.transcript.lastEventAt ?? $0.modifiedAt) >= sessionStart }
        let roots = recent.filter { !$0.transcript.isSubagent && !$0.transcript.isInternal }
        var reportedPaths = Set(recent.map(\.path))
        for root in roots { reportedPaths.formUnion(descendants(of: root.transcript.id!).map(\.path)) }
        let reported = indexed.sessions.filter { reportedPaths.contains($0.path) }
        let terminalOrigins = await readTerminalOrigins(Set(reported.compactMap(\.transcript.id)))
        let origins = sessionOriginsDirectory.map { CodexSessionOrigins.read(directory: $0) } ?? [:]
        func navigationTarget(for transcript: CodexTranscript) -> SessionNavigationTarget? {
            if let id = transcript.id {
                if let origin = origins[id] {
                    return origin.host == .daemon ? terminalOrigins[id] : origin.target
                }
                if let target = terminalOrigins[id] { return target }
            }
            return transcript.navigationTarget
        }
        let selected = accountReadings
        let native = readings[home]
        let windows = selected.flatMap { source, reading in
            reading.limits.rows(home: source).map { (row: $0, reading: reading) }
        }
        let models = Set(indexed.sessions.flatMap(\.transcript.models)).sorted()
        let consumers = models.map { AgentDescriptor(id: "codex-model:\($0)", vendor: "Codex", model: ModelCatalog.consumerName(of: "codex-model:\($0)"),
                                                     source: L10n.sourceCodexAppServer, enabled: true) }
        let snapshots = windows.map { row, reading in
            UsageSnapshot(agentId: row.id, remainingPct: row.window.remainingPct, weeklyRemainingPct: row.weekly?.remainingPct,
                          resetAt: row.window.resetAt, windowDuration: row.window.duration,
                          weeklyResetAt: row.weekly?.resetAt, updatedAt: reading.at)
        }
        var byAgent: [String: UsageInsights] = [:]
        for snapshot in snapshots {
            let samples = await history.samples(agentId: snapshot.agentId, since: QuotaMath.historyStart(for: snapshot, now: now))
            byAgent[snapshot.agentId] = QuotaMath.insights(snapshot: snapshot, samples: samples, now: now)
        }
        func liveSession(_ session: CodexTranscriptStore.Session, ancestors: Set<String> = []) -> LiveSession {
            let t = session.transcript, id = t.id!, evidence = t.evidence
            let live = SessionPhase.read(evidence, rule: .rollout, at: now).inFlight
            let seen = ancestors.union([id])
            let agents = (children[id] ?? []).filter {
                !$0.transcript.isInternal && !seen.contains($0.transcript.id!)
            }.sorted {
                let a = $0.transcript.startedAt ?? $0.modifiedAt, b = $1.transcript.startedAt ?? $1.modifiedAt
                return a == b ? $0.transcript.id! < $1.transcript.id! : a < b
            }.map { liveSession($0, ancestors: seen) }
            return LiveSession(id: id, agentId: "codex-model:\(t.model)",
                               task: session.title ?? t.task ?? t.agentPath.map { URL(fileURLWithPath: $0).lastPathComponent }
                                   ?? t.agentName ?? t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Codex",
                               terminal: t.cwd.map { URL(fileURLWithPath: $0).lastPathComponent },
                               startedAt: t.startedAt ?? session.modifiedAt,
                               endedAt: live ? nil : (t.lastActivityAt ?? session.modifiedAt),
                               pctOfWindow: nil, tokensIn: t.inputTokens,
                               tokensOut: t.outputTokens, client: t.client, transcriptPath: session.path,
                               cacheReadTokens: t.cachedInputTokens, observedAt: evidence.newest, workingDirectory: t.cwd,
                               subagentTranscripts: descendants(of: id).map(\.path).sorted(), agentName: t.agentName, subagentSessions: agents,
                               lastActivityAt: t.lastEventAt, navigationTarget: navigationTarget(for: t),
                               activity: t.isInternal ? nil : t.activity ?? SessionActivity())
        }
        let sessions = roots.map { session in
            (session: session, live: SessionPhase.read(session.transcript.evidence, rule: .rollout, at: now).inFlight)
        }.sorted { a, b in
            if a.live != b.live { return a.live }
            return (a.session.transcript.lastActivityAt ?? .distantPast) > (b.session.transcript.lastActivityAt ?? .distantPast)
        }.map { liveSession($0.session) }
        // An unread login's failure belongs to its client home, including accounts retained from an earlier run.
        let selectedHomes = Set(selected.map(\.0))
        let unread = failures.filter { readings[$0.key] == nil && !selectedHomes.contains($0.key) }
        let clientFailures = Dictionary(uniqueKeysWithValues: unread.map {
            (ClientHome.sourceKey(provider: "Codex", home: $0.key), $0.value)
        })
        let messages = (selected.compactMap { failures[$0.0] } + Array(unread.values)).sorted()
        let notice = messages.isEmpty ? nil : messages.joined(separator: " · ")
        let failed = selected.isEmpty ? failures[home].map { ["Codex": $0] } ?? [:] : [:]
        let sourceNotices = failed.merging(clientFailures, uniquingKeysWith: { _, new in new })
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
                client: clients[reading.limits.providerAccount(home: source).id]?.client,
                label: reading.limits.account?.email, plan: reading.limits.plan, observedAt: reading.at,
                quotaNotice: failures[source], readingIssue: failures[source].map(ReadingIssue.readFailed),
                resetCredits: reading.limits.rateLimitResetCredits,
                aliases: reading.limits.partialKeys,
                quotaWindowIDs: Set(reading.limits.rows(home: source).map(\.id)))
        }
        let completions = reported.flatMap { session in
            (session.transcript.completions ?? []).map { completion in
                var named = completion
                named.task = session.title ?? completion.task
                named.navigationTarget = navigationTarget(for: session.transcript)
                return named
            }
        }
        return UsageReport(generatedAt: now, snapshots: snapshots, sessions: sessions,
                           notice: notice, discoveredAgents: windows.map { $0.row.descriptor }, consumers: consumers,
                           indexing: indexed.indexing, insightsByAgent: byAgent,
                           subscriptions: native?.limits.plan.map { ["Codex": $0] } ?? [:],
                           sourceNotices: sourceNotices, quotaNotices: sourceNotices, readingIssues: sourceNotices.mapValues(ReadingIssue.readFailed),
                           consumerIdsByQuota: consumerIdsByQuota, codexResetCredits: selected.count == 1 ? selected.first?.1.limits.rateLimitResetCredits : nil,
                           codexResetCreditsObservedAt: selected.count == 1 && selected.first?.1.limits.rateLimitResetCredits != nil ? selected.first?.1.at : nil,
                           completions: completions,
                           turns: reported.flatMap { $0.transcript.sessionTurns },
                           accounts: lastRequestAt == nil || selected.isEmpty && !failures.isEmpty ? nil : ["Codex": observations])
    }
}
