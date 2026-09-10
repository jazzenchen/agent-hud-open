import Foundation

/// Scans `~/.claude/projects` for transcripts and keeps per-file accumulators so growing files are read
/// incrementally instead of re-parsed on every poll. Indexing is cooperative: each call reads newest files first
/// for at most `timeBudget` seconds and reports how many are still pending, so the UI shows data right away.
/// Accumulators are persisted, so a restart only reads files that changed since the last run.
public actor ClaudeTranscriptStore {
    private struct Entry: Codable {
        var mtime: Date
        var size: Int
        var offset: Int
        var accumulator: TranscriptAccumulator
    }

    /// Work done by the last `sessions(modifiedSince:)` call, for diagnostics.
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

    /// v3: rebuild both the corrected role index and completion timestamps from all message blocks.
    /// v5: retain the explicit current turn, including terminal state and its original start time.
    public static var defaultCacheURL: URL {
        AppSupport.directory.appendingPathComponent("transcripts-cache-v5.json")
    }

    public static let defaultTimeBudget: TimeInterval = 1.5

    private let roots: [URL]
    private let cacheURL: URL?
    private var entries: [String: Entry] = [:]
    private var sessionsByPath: [String: TranscriptSession] = [:]
    private var dirty = false
    private var lastSavedAt = Date.distantPast
    private let cacheSaveInterval: TimeInterval
    public private(set) var lastScan = ScanStats()

    public init(roots: [URL] = ClaudeTranscriptStore.defaultRoots, cacheURL: URL? = nil, cacheSaveInterval: TimeInterval = 30) {
        self.roots = roots
        self.cacheURL = cacheURL
        self.cacheSaveInterval = cacheSaveInterval
        if let cacheURL, let data = try? Data(contentsOf: cacheURL) {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .secondsSince1970
            entries = (try? decoder.decode([String: Entry].self, from: data)) ?? [:]
        }
    }

    public init(root: URL) {
        self.init(roots: [root], cacheURL: nil)
    }

    /// Sessions whose transcript file was modified at or after `cutoff`. Blocks until the index is complete.
    public func sessions(modifiedSince cutoff: Date) -> [TranscriptSession] {
        index(modifiedSince: cutoff, timeBudget: .infinity).sessions
    }

    /// One cooperative indexing step: newest changed files first, bounded by `timeBudget`.
    public func index(modifiedSince cutoff: Date, timeBudget: TimeInterval = ClaudeTranscriptStore.defaultTimeBudget) -> Result {
        let started = Date()
        lastScan = ScanStats()
        defer { lastScan.elapsed = Date().timeIntervalSince(started) }

        struct Candidate {
            let url: URL
            let mtime: Date
            let size: Int
        }
        let keys: [URLResourceKey] = [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]
        var seen: Set<String> = []
        var pending: [Candidate] = []
        var failures = 0
        for root in roots {
            // An unreadable tree is not an empty authoritative snapshot of the user's history.
            guard FileManager.default.fileExists(atPath: root.path) else { continue }
            guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles], errorHandler: { _, _ in
                failures += 1
                return true
            }) else {
                failures += 1
                continue
            }
            for case let url as URL in enumerator {
                guard url.pathExtension == "jsonl" else { continue }
                guard let values = try? url.resourceValues(forKeys: Set(keys)),
                      let mtime = values.contentModificationDate,
                      let size = values.fileSize,
                      values.isRegularFile != nil else { failures += 1; continue }
                guard values.isRegularFile == true, mtime >= cutoff else { continue }
                seen.insert(url.path)
                lastScan.files += 1
                if let entry = entries[url.path], entry.size == size, abs(entry.mtime.timeIntervalSince(mtime)) < 0.001 {
                    continue
                }
                pending.append(Candidate(url: url, mtime: mtime, size: size))
            }
        }
        pending.sort { $0.mtime > $1.mtime }

        // Always make progress on the newest file, then keep going while the budget lasts.
        var remaining = 0
        for (index, candidate) in pending.enumerated() {
            if index > 0, Date().timeIntervalSince(started) >= timeBudget {
                remaining = pending.count - index
                break
            }
            if !load(url: candidate.url, mtime: candidate.mtime, size: candidate.size) { failures += 1 }
        }

        let stale = failures == 0 ? entries.keys.filter { !seen.contains($0) } : []
        if !stale.isEmpty {
            for key in stale {
                entries.removeValue(forKey: key)
                sessionsByPath.removeValue(forKey: key)
            }
            dirty = true
        }
        saveIfNeeded()

        var result: [TranscriptSession] = []
        result.reserveCapacity(seen.count)
        for key in seen {
            if let cached = sessionsByPath[key] {
                result.append(cached)
            } else if let entry = entries[key], let built = entry.accumulator.build() {
                sessionsByPath[key] = built
                result.append(built)
            }
        }
        lastScan.pending = remaining + failures
        return Result(sessions: result, pending: lastScan.pending)
    }

    private func load(url: URL, mtime: Date, size: Int) -> Bool {
        let key = url.path
        let now = Date()
        if var entry = entries[key] {
            if size >= entry.offset, let (events, offset) = try? readEvents(url: url, from: entry.offset) {
                entry.accumulator.ingest(events)
                entry.offset = offset
            } else if let fresh = parseWhole(url: url) {
                entry.accumulator = fresh.accumulator
                entry.offset = fresh.offset
            } else {
                return false
            }
            entry.mtime = mtime
            entry.size = size
            entry.accumulator.compactIfFinished(now: now)
            entries[key] = entry
            sessionsByPath[key] = entry.accumulator.build()
            dirty = true
            return true
        }
        guard var fresh = parseWhole(url: url) else { return false }
        fresh.accumulator.compactIfFinished(now: now)
        entries[key] = Entry(mtime: mtime, size: size, offset: fresh.offset, accumulator: fresh.accumulator)
        sessionsByPath[key] = fresh.accumulator.build()
        dirty = true
        return true
    }

    private func parseWhole(url: URL) -> (accumulator: TranscriptAccumulator, offset: Int)? {
        guard let (events, offset) = try? readEvents(url: url, from: 0) else { return nil }
        var accumulator = TranscriptAccumulator(path: url.path, isSubagent: Self.isSubagent(url))
        accumulator.ingest(events)
        return (accumulator, offset)
    }

    /// Reads complete lines from `offset` in 4 MB chunks (transcripts can be hundreds of MB);
    /// returns the events and the offset just past the last newline consumed.
    private func readEvents(url: URL, from offset: Int) throws -> ([TranscriptEvent], Int) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        try handle.seek(toOffset: UInt64(offset))
        lastScan.filesRead += 1
        var events: [TranscriptEvent] = []
        var carry = Data()
        var consumed = 0
        let chunkSize = 4 << 20
        while let chunk = try handle.read(upToCount: chunkSize), !chunk.isEmpty {
            lastScan.bytesRead += chunk.count
            carry.append(chunk)
            guard let lastNewline = carry.lastIndex(of: 0x0A) else { continue }
            let complete = carry[carry.startIndex...lastNewline]
            events += FastTranscriptParser.parse(complete)
            consumed += complete.count
            carry = Data(carry[carry.index(after: lastNewline)...])
        }
        return (events, offset + consumed)
    }

    private func saveIfNeeded() {
        guard dirty, let cacheURL, Date().timeIntervalSince(lastSavedAt) >= cacheSaveInterval else { return }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(entries) else { return }
        do {
            try FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: cacheURL, options: .atomic)
            dirty = false
            lastSavedAt = Date()
        } catch { /* Keep dirty; the next poll retries. Unsaved offsets are recovered from the transcripts. */ }
    }

    static func isSubagent(_ url: URL) -> Bool {
        url.lastPathComponent.hasPrefix("agent-") || url.pathComponents.contains("subagents")
    }
}
