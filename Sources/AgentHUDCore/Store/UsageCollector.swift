import Foundation

/// A host's extension points in the collection pipeline. Every hook runs on the main actor inside the pass that fetched
/// the report, while no provider reads and nothing writes the usage ledger, so a hook may read the ledger itself.
public struct UsageCollectionHooks {
    /// Hourly buckets providers load, including the current partial hour; asked before every local poll and account step.
    public var historyHours: @MainActor () -> Int
    /// Receives each report the provider returned, before it is displayed. The pass, and so the next pass, waits for it.
    public var publish: (@MainActor (UsageReport) async -> Void)?
    /// Turns the provider's report into the displayed report, for example to add other data. The result must keep the
    /// provider's discovered agents, accounts, active quota pools, completions and turns, which drive the agent list and
    /// the island.
    public var merge: (@MainActor (UsageReport) async -> UsageReport)?

    public init(historyHours: @escaping @MainActor () -> Int = { UsageStore.historyHours },
                publish: (@MainActor (UsageReport) async -> Void)? = nil,
                merge: (@MainActor (UsageReport) async -> UsageReport)? = nil) {
        self.historyHours = historyHours
        self.publish = publish
        self.merge = merge
    }
}

/// The collection pipeline. Every source only signals that it has new data: a file change under its directories, a finished
/// account step, one of its checks falling due, or its poll interval when it cannot name its directories. The collector
/// waits for those signals and reads the signalled sources, one read or account step at a time, handing each report to
/// the store. A source's account steps run when its own work, or one of its windows, makes a new reading worth taking.
@MainActor
final class UsageCollector {
    weak var store: UsageStore?
    private let provider: any UsageProvider
    private let settings: SettingsStore
    private let hooks: UsageCollectionHooks
    private var pollTask: Task<Void, Never>?
    /// Wakes the waiting loop: a file change, a timer, a refresh.
    private var wake: AsyncStream<Void>.Continuation?
    /// One pass of the pipeline runs at a time; a request during a pass is served by the next one.
    private var isCollecting = false
    /// Account steps left in the current sweep, run one at a time between local reads.
    private var accountSteps: [(source: String, run: AccountRefreshStep)] = []
    /// When each source's account steps last ran; a source that has never run them is due at once.
    private var accountRunAt: [String: Date] = [:]
    /// Consent to read an account, and a look at the numbers, read every account at once instead of when work moves them.
    private var sweptWithCopilotQuota: Bool?
    private var forcesAccounts = false
    /// The read of every local source that stands in for a file event the watch missed.
    private var fallbackReadAt: Date?
    private var sources: [UsageSource] = []
    private var fetchedAt: Date?
    private var fetchedAgents: [AgentDescriptor]?
    /// Every source must be read: at start, at the fallback read, after a refresh request or a changed agent list.
    private var needsFetch = true
    /// Sources signalled since their last read.
    private var signalled: Set<String> = []
    /// When each source was last read, so a check or poll interval counts from that source's own read.
    private var readAt: [String: Date] = [:]
    /// A failed read is tried again at this time.
    private var retryAt: Date?
    private var changes: FileChangeMonitor?
    /// The provider's last report before the merge hook, numbered so a slower merge never replaces a newer one.
    private var local: (report: UsageReport, generation: Int)?
    private var generation = 0
    private var isMerging = false
    private var mergeRequested = false

    init(provider: any UsageProvider, settings: SettingsStore, hooks: UsageCollectionHooks) {
        self.provider = provider
        self.settings = settings
        self.hooks = hooks
        sources = provider.sources
    }

    func start() {
        stop()
        let (stream, continuation) = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        wake = continuation
        let directories = sources.compactMap(\.directories).flatMap { $0 }
        changes = directories.isEmpty ? nil : FileChangeMonitor(directories: directories, onChange: { continuation.yield() })
        pollTask = Task { [weak self] in
            var wakes = stream.makeAsyncIterator()
            while !Task.isCancelled {
                guard let pause = await self?.collect(force: false) else { return }
                // A cancelled timer must not wake the loop: only a timer that ran out yields.
                let timer = Task {
                    do { try await Task.sleep(for: .seconds(pause)) } catch { return }
                    continuation.yield()
                }
                guard await wakes.next() != nil else { timer.cancel(); return }
                timer.cancel()
            }
        }
    }

    func stop() {
        pollTask?.cancel()
        pollTask = nil
        wake?.finish()
        wake = nil
        changes = nil
        // Steps that did not run keep their source due, so a restart reads the accounts it owed.
        accountSteps = []
    }

    func refresh() async {
        // An explicit refresh is also a request to reconcile files even if a directory event is still in flight.
        await provider.fileChanges(nil)
        needsFetch = true
        _ = await collect(force: true)
        wake?.yield()
    }

    /// Reads every source's accounts in the next pass, whatever its work is doing; the request spacing still holds.
    func refreshAccounts() async {
        forcesAccounts = true
        await refresh()
    }

    /// A report installed directly is not the provider's; merging must not replace it.
    func forgetLocalReport() {
        local = nil
    }

    /// While nothing is signalled and file changes are watched, no change was missed: the data on screen is current.
    func confirmQuiet(at date: Date) {
        guard let store, !isCollecting, !needsFetch, signalled.isEmpty, retryAt == nil, changes?.isWatching != false,
              sources.allSatisfy({ $0.directories != nil }), date.timeIntervalSince(store.dataDate) >= 60 else { return }
        store.checkedAt = date
    }

    /// Merges the last provider report again without collecting. A local read in progress merges for it, or merges again
    /// once it returns when its own merge had already started.
    func remerge() async {
        guard hooks.merge != nil else { return }
        mergeRequested = true
        guard let store, !isMerging, !store.isRefreshing else { return }
        isMerging = true
        defer { isMerging = false }
        while mergeRequested, !store.isRefreshing, let merge = hooks.merge, let local {
            mergeRequested = false
            let merged = await merge(local.report)
            guard local.generation == self.local?.generation, store.isAccessAllowed else { continue }
            store.merged(merged)
        }
    }

    /// One pass: collects the signals, reads the signalled sources, then runs account steps within their budget.
    /// Returns how long to wait for the next signal that is known in advance; file changes wake the loop sooner.
    private func collect(force: Bool) async -> TimeInterval {
        guard let store, store.isAccessAllowed, !isCollecting else { return UsageRefresh.pollInterval }
        if let pausedUntil = store.pausedUntil, pausedUntil > Date() {
            return min(pausedUntil.timeIntervalSinceNow, UsageRefresh.accountInterval)
        }
        store.pausedUntil = nil
        isCollecting = true
        defer { isCollecting = false }
        let started = Date()
        if fallbackReadAt.map({ started.timeIntervalSince($0) >= UsageRefresh.accountInterval }) ?? true {
            fallbackReadAt = started
            // The fallback read catches a change the watch missed and picks up directories created since the last one.
            changes?.update()
            needsFetch = true
        }
        let consent = settings.settings.readCopilotQuota
        if accountSteps.isEmpty {
            // Consent to read an account follows the switch at once; a look reads whatever the request spacing allows;
            // otherwise every source waits until its own work, or one of its windows, is worth a reading.
            let consented = sweptWithCopilotQuota != consent, looked = forcesAccounts
            forcesAccounts = false
            sweptWithCopilotQuota = consent
            let due = await accountDue(at: started)
            accountSteps = sources.filter { source in
                if consented { return true }
                let spaced = (accountRunAt[source.name] ?? .distantPast).addingTimeInterval(UsageRefresh.accountRequestSpacing)
                return spaced <= started && (looked || due[source.name].map { $0 <= started } ?? false)
            }.flatMap { source in source.accountSteps.map { (source.name, $0) } }
        }
        await gatherSignals(at: started)
        if changes?.isWatching == false { await provider.fileChanges(nil) }
        let spacing = force ? 0 : fetchedAt.map { UsageRefresh.readSpacing - started.timeIntervalSince($0) } ?? 0
        if needsFetch || !signalled.isEmpty {
            if spacing <= 0 {
                let names: Set<String>? = needsFetch || signalled.contains("") ? nil : signalled
                await fetch(at: started, sources: names, into: store)
                if mergeRequested { await remerge() }
            }
        } else if started.timeIntervalSince(store.dataDate) >= 60, changes?.isWatching != false {
            store.checkedAt = started
        }
        if !accountSteps.isEmpty, !Task.isCancelled {
            let stepsStarted = Date()
            repeat {
                let step = accountSteps.removeFirst()
                accountRunAt[step.source] = Date()
                await step.run(hooks.historyHours())
                if step.source.isEmpty { needsFetch = true } else { signalled.insert(step.source) }
            } while !accountSteps.isEmpty && !Task.isCancelled && Date().timeIntervalSince(stepsStarted) < UsageRefresh.accountStepBudget
        }
        return await pause(after: Date())
    }

    /// Turns what happened since the last pass into signalled sources.
    private func gatherSignals(at date: Date) async {
        if fetchedAgents != settings.agents { needsFetch = true }
        if let retryAt, retryAt <= date { needsFetch = true; self.retryAt = nil }
        if store?.isIndexing == true, fetchedAt.map({ date.timeIntervalSince($0) >= UsageRefresh.indexingInterval }) ?? true {
            needsFetch = true
        }
        if let changes, changes.isWatching {
            if let paths = changes.consumePaths() {
                if !paths.isEmpty {
                    await provider.fileChanges(paths)
                    signalled.formUnion(owners(of: paths))
                }
            } else {
                await provider.fileChanges(nil)
                needsFetch = true
            }
        }
        for source in polledSources where readAt[source].map({ date.timeIntervalSince($0) >= UsageRefresh.pollInterval }) ?? true {
            signalled.insert(source)
        }
        for (source, times) in await checks() {
            let since = readAt[source] ?? .distantPast
            if times.contains(where: { $0 > since && $0 <= date }) { signalled.insert(source) }
        }
    }

    /// Sources read on a schedule: those without directories, and every source while file changes cannot be watched.
    private var polledSources: [String] {
        sources.filter { $0.directories == nil || changes?.isWatching == false }.map(\.name)
    }

    /// The sources whose directories contain a changed path. File events name real paths, such as `/private/var` for
    /// `/var`, which Foundation's own path normalization hides, so each directory is compared as given and as `realpath` resolves it.
    private func owners(of paths: Set<String>) -> Set<String> {
        var owners: Set<String> = []
        for source in sources {
            guard let directories = source.directories else { continue }
            let prefixes = directories.flatMap { url -> [String] in
                let path = url.standardizedFileURL.path
                guard let real = realpath(path, nil) else { return [path] }
                defer { free(real) }
                let resolved = String(cString: real)
                return path == resolved ? [path] : [path, resolved]
            }
            if paths.contains(where: { path in prefixes.contains { path == $0 || path.hasPrefix($0 + "/") } }) {
                owners.insert(source.name)
            }
        }
        return owners
    }

    /// When each source's account steps are next worth running: what the provider asks for, the account interval when
    /// it names no time, and at once for a source that has never run them. No source runs twice within the request
    /// spacing, so a provider that wants a reading sooner than it allows one is asked once the spacing has passed.
    private func accountDue(at now: Date) async -> [String: Date] {
        let asked = await provider.accountChecks(since: accountRunAt, now: now)
        return sources.reduce(into: [:]) { due, source in
            guard let last = accountRunAt[source.name] else { return due[source.name] = .distantPast }
            due[source.name] = max(asked[source.name] ?? last.addingTimeInterval(UsageRefresh.accountInterval),
                                   last.addingTimeInterval(UsageRefresh.accountRequestSpacing))
        }
    }

    /// Each source's check times; a provider that does not split itself is checked from the report it returned.
    private func checks() async -> [String: [Date]] {
        let checks = await provider.sourceChecks()
        guard checks.isEmpty, sources.count == 1, let report = local?.report else { return checks }
        return [sources[0].name: report.activityChecks]
    }

    /// The time until the next signal known in advance: an account step, a source's next account reading, the fallback
    /// read, a pending read held back by the read spacing, indexing, a poll interval, a check or a retry.
    private func pause(after now: Date) async -> TimeInterval {
        var times: [Date] = []
        if !accountSteps.isEmpty { times.append(now.addingTimeInterval(UsageRefresh.accountStepBudget)) }
        else { times += await accountDue(at: now).values }
        if let fallbackReadAt { times.append(fallbackReadAt.addingTimeInterval(UsageRefresh.accountInterval)) }
        if needsFetch || !signalled.isEmpty { times.append((fetchedAt ?? now).addingTimeInterval(UsageRefresh.readSpacing)) }
        if store?.isIndexing == true { times.append((fetchedAt ?? now).addingTimeInterval(UsageRefresh.indexingInterval)) }
        if let retryAt { times.append(retryAt) }
        for source in polledSources { times.append((readAt[source] ?? now).addingTimeInterval(UsageRefresh.pollInterval)) }
        for (_, dates) in await checks() { times += dates.filter { $0 > now } }
        let next = times.min() ?? now.addingTimeInterval(UsageRefresh.accountInterval)
        return max(0, next.timeIntervalSince(now))
    }

    /// The publish and merge hooks run here, before the pass can end.
    private func fetch(at date: Date, sources names: Set<String>?, into store: UsageStore) async {
        needsFetch = false
        store.isRefreshing = true
        defer { store.isRefreshing = false }
        do {
            let fetched = try await provider.fetchUsage(agents: settings.agents, historyHours: hooks.historyHours(), sources: names)
            let read = names ?? Set(sources.map(\.name))
            signalled.subtract(read)
            for name in read { readAt[name] = date }
            await hooks.publish?(fetched)
            mergeRequested = false
            let report = await hooks.merge?(fetched) ?? fetched
            guard store.isAccessAllowed, !Task.isCancelled else { needsFetch = true; return }
            settings.mergeDiscovered(report.discoveredAgents, activeQuotaPoolIDs: report.activeQuotaPoolIDs, accounts: report.accounts, replaceQuotaWindows: true)
            generation += 1
            local = (fetched, generation)
            store.collected(report)
            fetchedAt = date
            fetchedAgents = settings.agents
        } catch {
            // Every source failed; the retry reads them all again instead of spinning on the same signals.
            signalled = []
            retryAt = date.addingTimeInterval(UsageRefresh.pollInterval)
            guard store.isAccessAllowed, !Task.isCancelled else { return }
            fetchedAt = date
            store.lastError = error.localizedDescription
        }
        store.now = Date()
    }
}
