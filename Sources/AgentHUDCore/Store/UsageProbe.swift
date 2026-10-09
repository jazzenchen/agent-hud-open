import Foundation

/// `--probe`: reads the way the application does and keeps nothing, then prints what it found. Its ledger lives in
/// memory, so it writes nothing to the usage ledger, removes or imports no earlier version's files and remembers no
/// account identity. It prints no conversation text and no credential.
public enum UsageProbe {
    /// The hours of history the probe reads.
    public static let historyHours = 48

    /// Prints where the clients' engines and data are, one step of reading Claude Code's transcripts, and what one account
    /// refresh and one report of the agents in `settings` found, with how long each took. Returns the exit status: 0
    /// once a report was read, 1 when reading it failed.
    @MainActor
    public static func run(settings: SettingsStore) async -> Int32 {
        print("Claude Code engine: \(ClaudeEngineLocator.find()?.path ?? "not found")")
        print("Codex engine: \(CodexLocator.find()?.path ?? "not found")")
        print("Codex data: \(CodexLocator.dataDirectory.path)")
        print("DeepSeek data: \(DeepSeekLocator.dataDirectory.path) (\(DeepSeekLocator.isInstalled() ? "installed" : "not detected"))")
        print("DeepSeek decoder: \(DeepSeekLocator.nodeExecutable()?.path ?? "Node.js not found")")
        let agents = settings.agents.map { $0.enabled ? $0.id : $0.id + " (off)" }
        print("Agents: \(agents.isEmpty ? "none" : agents.joined(separator: ", "))")
        // A ledger of its own, so the report below reads the transcripts as the application does.
        let transcripts = ClaudeTranscriptStore(ledger: .inMemory())
        let scanned = Date()
        let step = await transcripts.index(modifiedSince: scanned.addingTimeInterval(-7 * 86400))
        let scan = await transcripts.lastScan
        print("Claude Code transcripts, one step: \(scan.files) files, \(scan.filesRead) read (\(megabytes(scan.bytesRead))) in "
              + "\(seconds(since: scanned)); \(step.sessions.count) sessions ready, \(step.pending) pending")
        let provider = CombinedUsageProvider.standard(settings: settings, ledger: .inMemory(), persistent: false)
        let refreshed = Date()
        await provider.refreshAccountUsage(historyHours: historyHours)
        print("Account refresh: \(seconds(since: refreshed))")
        let started = Date()
        let report: UsageReport
        do {
            report = try await provider.fetchUsage(agents: settings.agents, historyHours: historyHours)
        } catch {
            FileHandle.standardError.write(Data("Usage probe failed: \(error.localizedDescription)\n".utf8))
            return 1
        }
        let now = Date()
        print("Report: \(seconds(since: started))")
        print("Quota windows: \(report.snapshots.count); sessions: \(report.sessions.count); live: \(report.sessions.filter(\.isLive).count); billing accounts: \(report.billing.count)")
        print("Plans: \(report.subscriptions.sorted { $0.key < $1.key }.map { "\($0.key) \($0.value)" }.joined(separator: ", ")); notice: \(report.notice ?? "-")")
        print("Discovered: \(report.discoveredAgents.map { "\($0.id)=\($0.model)" }.joined(separator: ", "))")
        for snapshot in report.snapshots {
            let reset = snapshot.resetAt.map { Countdown.until($0, now: now) } ?? "-"
            let weekly = snapshot.weeklyRemainingPct.map { "\(Int($0))%" } ?? "-"
            let used = snapshot.remainingPct.map { "\(Int(100 - $0))%" } ?? "N/A"
            print("  \(snapshot.agentId): used \(used), reset \(reset), weekly remaining \(weekly)")
        }
        let clients = Dictionary(grouping: report.sessions, by: { $0.client ?? "unknown" }).mapValues(\.count)
        print("Session clients: \(clients.sorted { $0.key < $1.key }.map { "\($0.key) \($0.value)" }.joined(separator: ", "))")
        let deepseek = report.sessions.filter { $0.client == "DeepSeek Harness" }
        let deepseekTokens = report.usage.filter { $0.agentId.hasPrefix("deepseek-model:") }.reduce(0) { $0 + $1.total }
        print("DeepSeek: \(deepseek.count) sessions (\(deepseek.filter(\.isLive).count) live), \(deepseekTokens) tokens in range")
        for account in report.billing {
            print("\(account.vendor) balances: \(account.balances.map { MoneyFormat.amount($0.total, currency: $0.currency) }.joined(separator: ", "))")
            let cost = account.estimatedCost(currency: account.currency).map { MoneyFormat.amount($0, currency: account.currency, estimated: true) }
            print("\(account.vendor) estimated cost: \(cost ?? "unpriced")")
        }
        for session in report.sessions.prefix(5) {
            print("  \(session.isLive ? "●" : "○") \(session.agentId) \(session.client ?? "-") \(TokenFormat.inOut(in: session.tokensIn, out: session.tokensOut))")
        }
        for (kind, values) in VendorCatalog.unnamed.sorted(by: { $0.key < $1.key }) {
            print("Unnamed \(kind): \(values.sorted().joined(separator: ", "))")
        }
        return 0
    }

    private static func seconds(since start: Date) -> String {
        String(format: "%.1f s", Date().timeIntervalSince(start))
    }

    private static func megabytes(_ bytes: Int) -> String {
        String(format: "%.1f MB", Double(bytes) / 1_048_576)
    }
}
