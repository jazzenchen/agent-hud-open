import Foundation

/// Incremental metadata reader for plaintext and concatenated Zstandard-frame Harness logs over the usage ledger.
/// Each log is one ledger contribution; when a session's log exists in several places, only its newest copy counts.
public actor DeepSeekTranscriptStore {
    public struct Session: Sendable {
        public let transcript: DeepSeekTranscript
        public let modifiedAt: Date
        public let path: String
    }

    public struct Result: Sendable {
        public let sessions: [Session]
        public let indexing: IndexProgress?
        public let notice: String?
    }

    let root: URL
    private let ledger: UsageLedger
    private let logs: TailLogStore<HarnessLogs>

    public init(root: URL, ledger: UsageLedger = .inMemory()) {
        self.root = root
        self.ledger = ledger
        // Harness names each format generation `session[.vN].jsonl[.zstd]` and keeps older generations beside their successor.
        logs = TailLogStore(roots: [root], ledger: ledger, watchesChanges: false) {
            $0.lastPathComponent.wholeMatch(of: /session(\.v[1-9][0-9]*)?\.jsonl(\.zstd)?/) != nil
        }
    }

    /// DeepSeek's 15-minute token totals from the period holding `since`.
    public func usage(since: Date) async -> [UsageBucket] {
        (try? await ledger.buckets(since: since, source: HarnessLogs.source)) ?? []
    }

    /// Estimated cost periods of the Harness account, and each log's estimate keyed by path.
    public func costs(since: Date) async -> (buckets: [CostBucket], logs: [String: [String: Decimal]]) {
        let buckets = (try? await ledger.costBuckets(since: since))?["DeepSeek"] ?? []
        return (buckets, (try? await ledger.contributionCosts(source: HarnessLogs.source)) ?? [:])
    }

    public func index(since cutoff: Date, timeBudget: TimeInterval = 1.5) async -> Result {
        let pass = await logs.index(since: cutoff, timeBudget: timeBudget)
        var notice = pass.gaps.unreadable > 0 ? L10n.text("无法读取部分 Harness 会话", "Some Harness sessions could not be read") : nil
        if let error = pass.failures.last {
            notice = L10n.text("Harness 会话读取失败：", "Harness session read failed: ") + error.localizedDescription
        }
        // Session ids own usage even if a log has been copied between project directories.
        var sessions: [String: Session] = [:]
        for log in pass.logs.sorted(by: { $0.file.modified > $1.file.modified }) {
            guard let entry = logs.entry(log.path), let id = entry.summary.id, sessions[id] == nil else { continue }
            sessions[id] = Session(transcript: entry.summary, modifiedAt: entry.modified, path: log.path)
        }
        return Result(sessions: sessions.values.sorted { $0.transcript.id! < $1.transcript.id! },
                      indexing: pass.pending > 0 ? IndexProgress(done: pass.logs.count - pass.pending, total: pass.logs.count) : nil, notice: notice)
    }
}

/// Harness session logs; a log copied into another project directory counts once.
enum HarnessLogs: TailLog {
    static let source = "deepseek"
    static let summaryKey = "transcript"
    /// 2: cache writes, reasoning and turn starts.
    static let version = 2

    static func summary(for url: URL) -> DeepSeekTranscript { DeepSeekTranscript() }

    /// Compressed streams replay on change; the summary resumes at its decoded byte offset.
    static func contents(of url: URL) async throws -> Data? { try await DeepSeekLogReader.read(url) }

    static func ingest(_ lines: Data, into transcript: inout DeepSeekTranscript) throws -> [UsageLedger.Event] {
        for line in lines.split(separator: 0x0A) { try transcript.ingest(line) }
        return transcript.drainUsage()
    }

    static func drainMarks(_ transcript: inout DeepSeekTranscript) -> [UsageLedger.Mark] { transcript.drainMarks() }

    static func group(_ transcript: DeepSeekTranscript) -> String? { transcript.id ?? "" }
}
