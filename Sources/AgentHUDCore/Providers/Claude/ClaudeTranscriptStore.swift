import Foundation

/// Scans `~/.claude/projects` for transcripts and reads only the bytes appended since the last pass. Indexing is
/// cooperative: each call reads newest files first for at most `timeBudget` seconds and reports how many are still
/// pending, so the UI shows data right away. Parse positions and summaries live in the usage ledger, and each
/// transcript's token events are one ledger contribution, so a restart only reads files that changed.
public actor ClaudeTranscriptStore {
    /// Work done by the last `index` call, for diagnostics.
    public struct ScanStats: Hashable, Sendable {
        public var files = 0
        public var filesRead = 0
        public var bytesRead = 0
        public var pending = 0
        public var elapsed: TimeInterval = 0
    }

    public struct Result: Sendable {
        public let sessions: [TranscriptSession]
        /// Files still waiting to be (re)read; zero once the index is complete.
        public let pending: Int
    }

    /// Claude Code writes to `~/.claude/projects`; newer builds may use the XDG config directory instead.
    public static var defaultRoots: [URL] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return [
            home.appendingPathComponent(".claude/projects", isDirectory: true),
            home.appendingPathComponent(".config/claude/projects", isDirectory: true),
        ]
    }

    public static let defaultTimeBudget: TimeInterval = 1.5

    let roots: [URL]
    private let ledger: UsageLedger
    private let logs: TailLogStore<ClaudeTranscripts>
    private var sessionsByPath: [String: TranscriptSession] = [:]
    public private(set) var lastScan = ScanStats()

    /// - watchesChanges: after the first listing, polls look only at transcripts a directory watch reports changed.
    public init(roots: [URL] = ClaudeTranscriptStore.defaultRoots, ledger: UsageLedger = .inMemory(), watchesChanges: Bool = false) {
        self.roots = roots
        self.ledger = ledger
        logs = TailLogStore(roots: roots, ledger: ledger, watchesChanges: watchesChanges) { $0.pathExtension == "jsonl" }
    }

    public init(root: URL) {
        self.init(roots: [root])
    }

    public func fileChanges(_ paths: Set<String>?) { logs.noteChanges(paths) }

    /// Sessions whose transcript file was modified at or after `cutoff`. Blocks until the index is complete.
    public func sessions(modifiedSince cutoff: Date) async -> [TranscriptSession] {
        await index(modifiedSince: cutoff, timeBudget: .infinity).sessions
    }

    /// Input plus output tokens of each transcript at or after `since`, keyed by path.
    public func tokens(since: Date) async -> [String: Int] {
        (try? await ledger.tokens(source: ClaudeTranscripts.source, since: since)) ?? [:]
    }

    /// Claude's 15-minute token totals from the period holding `since`.
    public func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: ClaudeTranscripts.source)) ?? []
    }

    /// One cooperative indexing step: newest changed files first, bounded by `timeBudget`.
    public func index(modifiedSince cutoff: Date, timeBudget: TimeInterval = ClaudeTranscriptStore.defaultTimeBudget) async -> Result {
        let started = Date()
        let pass = await logs.index(since: cutoff, timeBudget: timeBudget)
        if pass.reloaded { sessionsByPath = [:] }
        for path in pass.changed + pass.removed { sessionsByPath[path] = nil }

        var result: [TranscriptSession] = []
        result.reserveCapacity(pass.logs.count)
        for log in pass.logs {
            if let cached = sessionsByPath[log.path] {
                result.append(cached)
            } else if let built = logs.entry(log.path)?.summary.build() {
                sessionsByPath[log.path] = built
                result.append(built)
            }
        }
        var scan = ScanStats()
        scan.files = pass.logs.count
        scan.filesRead = pass.filesRead
        scan.bytesRead = pass.bytesRead
        scan.pending = pass.pending + pass.failures.count
        scan.elapsed = Date().timeIntervalSince(started)
        lastScan = scan
        return Result(sessions: result, pending: scan.pending)
    }

    static func isSubagent(_ url: URL) -> Bool {
        url.lastPathComponent.hasPrefix("agent-") || url.pathComponents.contains("subagents")
    }
}

/// Claude Code transcripts; each counts on its own.
enum ClaudeTranscripts: TailLog {
    static let source = "claude"
    static let summaryKey = "accumulator"
    /// 2: cache writes, thinking, prompts and compactions.
    static let version = 2

    static func summary(for url: URL) -> TranscriptAccumulator {
        TranscriptAccumulator(path: url.path, isSubagent: ClaudeTranscriptStore.isSubagent(url))
    }

    static func ingest(_ lines: Data, into accumulator: inout TranscriptAccumulator) -> [UsageLedger.Event] {
        accumulator.ingest(FastTranscriptParser.parse(lines))
    }

    static func drainMarks(_ accumulator: inout TranscriptAccumulator) -> [UsageLedger.Mark] { accumulator.drainMarks() }

    static func finishRead(_ accumulator: inout TranscriptAccumulator, now: Date) {
        accumulator.compactIfFinished(now: now)
    }
}
