import Foundation

/// Incremental metadata cache for plaintext and concatenated Zstandard-frame Harness logs.
public actor DeepSeekTranscriptStore {
    private struct Entry: Codable {
        var modifiedAt: Date
        var size: Int
        var offset: Int
        var committedSize: Int
        var transcript: DeepSeekTranscript
    }

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

    private let root: URL
    private let cacheURL: URL?
    private var entries: [String: Entry] = [:]

    public init(root: URL, cacheURL: URL? = nil) {
        self.root = root; self.cacheURL = cacheURL
        if let cacheURL, let data = try? Data(contentsOf: cacheURL) {
            entries = (try? JSONDecoder().decode([String: Entry].self, from: data)) ?? [:]
        }
    }

    public func index(since cutoff: Date, timeBudget: TimeInterval = 1.5) -> Result {
        let deadline = Date().addingTimeInterval(timeBudget)
        let keys: Set<URLResourceKey> = [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]
        var candidates: [(url: URL, modified: Date, size: Int)] = []
        var notice: String?
        if FileManager.default.fileExists(atPath: root.path) {
            let files = FileManager.default.enumerator(at: root, includingPropertiesForKeys: Array(keys), options: [.skipsHiddenFiles], errorHandler: { _, _ in
                notice = L10n.text("无法读取部分 Harness 会话目录", "Some Harness session directories could not be read")
                return true
            })
            for case let url as URL in files ?? FileManager.DirectoryEnumerator() {
                guard ["session.jsonl", "session.jsonl.zstd"].contains(url.lastPathComponent) else { continue }
                do {
                    let values = try url.resourceValues(forKeys: keys)
                    guard values.isRegularFile == true, let modified = values.contentModificationDate, modified >= cutoff,
                          let size = values.fileSize else { continue }
                    candidates.append((url, modified, size))
                } catch { notice = L10n.text("无法读取部分 Harness 会话", "Some Harness sessions could not be read") }
            }
        }
        candidates.sort { $0.modified > $1.modified }
        let present = Set(candidates.map { $0.url.path })
        var dirty = entries.keys.contains { !present.contains($0) }
        entries = entries.filter { present.contains($0.key) }
        var pending = 0
        for candidate in candidates {
            let old = entries[candidate.url.path]
            if let old, old.size == candidate.size, old.modifiedAt == candidate.modified, old.offset == old.committedSize { continue }
            if Date() >= deadline { pending += 1; continue }
            do {
                // Compressed streams replay on change; the summary resumes at its decoded byte offset.
                let data = try DeepSeekLogReader.read(candidate.url)
                var entry = old ?? Entry(modifiedAt: candidate.modified, size: candidate.size, offset: 0, committedSize: 0, transcript: DeepSeekTranscript())
                if candidate.size < entry.size || data.count < entry.offset || (old?.size == candidate.size && old?.modifiedAt != candidate.modified) {
                    entry.offset = 0; entry.transcript = DeepSeekTranscript()
                }
                entry.committedSize = data.lastIndex(of: 0x0A).map { $0 + 1 } ?? 0
                while entry.offset < entry.committedSize, Date() < deadline,
                      let newline = data[entry.offset...].firstIndex(of: 0x0A) {
                    let line = Data(data[entry.offset..<newline])
                    if !line.isEmpty { try entry.transcript.ingest(line) }
                    entry.offset = newline + 1
                }
                entry.modifiedAt = candidate.modified; entry.size = candidate.size
                entries[candidate.url.path] = entry; dirty = true
                if entry.offset < entry.committedSize { pending += 1 }
            } catch {
                notice = L10n.text("Harness 会话读取失败：", "Harness session read failed: ") + error.localizedDescription
            }
        }
        if dirty, let cacheURL, let data = try? JSONEncoder().encode(entries) {
            try? FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? data.write(to: cacheURL, options: .atomic)
        }
        // Session ids own usage even if a log has been copied between project directories.
        var sessions: [String: Session] = [:]
        for candidate in candidates {
            guard let entry = entries[candidate.url.path], let id = entry.transcript.id, sessions[id] == nil else { continue }
            sessions[id] = Session(transcript: entry.transcript, modifiedAt: entry.modifiedAt, path: candidate.url.path)
        }
        return Result(sessions: sessions.values.sorted { $0.transcript.id! < $1.transcript.id! },
                      indexing: pending > 0 ? IndexProgress(done: candidates.count - pending, total: candidates.count) : nil, notice: notice)
    }
}
