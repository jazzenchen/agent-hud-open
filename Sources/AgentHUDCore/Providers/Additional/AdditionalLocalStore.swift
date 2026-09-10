import Foundation

actor AdditionalLocalStore {
    let source: AdditionalSource
    let roots: [URL]
    private struct Entry { let signature: String; let result: ProviderSessions }
    private var entries: [URL: Entry] = [:]

    static var grokHome: URL {
        ProcessInfo.processInfo.environment["GROK_HOME"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".grok")
    }
    static var geminiHome: URL {
        ProcessInfo.processInfo.environment["GEMINI_CLI_HOME"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".gemini")
    }
    init(source: AdditionalSource, roots: [URL]? = nil) {
        self.source = source
        if let roots { self.roots = roots; return }
        switch source {
        case .grok: self.roots = [Self.grokHome.appendingPathComponent("sessions"), Self.grokHome.appendingPathComponent("logs")]
        case .antigravity: self.roots = ["antigravity-cli/conversations", "antigravity", "antigravity/conversations"].map { Self.geminiHome.appendingPathComponent($0) }
        case .cursor: self.roots = []
        }
    }

    func index(since: Date) -> ProviderSessions {
        let started = Date(), manager = FileManager.default
        var candidates: [(url: URL, signature: String, modified: Date)] = [], seen = Set<URL>()
        var failed = false, visited = 0, overlapNotice: String?
        for root in roots where manager.fileExists(atPath: root.path) {
            guard let enumerator = manager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey],
                options: [.skipsHiddenFiles], errorHandler: { _, _ in failed = true; return false }) else { failed = true; continue }
            for case let url as URL in enumerator {
                visited += 1
                if visited > 20000 { failed = true; break }
                // Antigravity's recognized SQLite roots are flat; don't recursively scan its configuration/storage.
                if source == .antigravity, url.pathExtension != "db" { enumerator.skipDescendants(); continue }
                let accepted: Bool
                switch source {
                case .antigravity: accepted = url.pathExtension == "db"
                case .grok: accepted = url.lastPathComponent == "updates.jsonl" || url.lastPathComponent == "unified.jsonl"
                case .cursor: accepted = false
                }
                guard accepted else { continue }
                guard let attributes = try? url.resourceValues(forKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey]),
                      attributes.isRegularFile == true, let modified = attributes.contentModificationDate, let size = attributes.fileSize else { failed = true; continue }
                var lastModified = modified
                var signature = "\(modified.timeIntervalSince1970):\(size)"
                let related = source == .antigravity ? [URL(fileURLWithPath: url.path + "-wal")]
                    : source == .grok && url.lastPathComponent == "updates.jsonl"
                    ? ["summary.json", "signals.json"].map { url.deletingLastPathComponent().appendingPathComponent($0) } : []
                for sibling in related {
                    if let values = try? sibling.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]), let date = values.contentModificationDate {
                        lastModified = max(lastModified, date); signature += ":\(date.timeIntervalSince1970):\(values.fileSize ?? 0)"
                    }
                }
                guard lastModified >= since, seen.insert(url).inserted else { continue }
                candidates.append((url, signature, lastModified))
            }
        }
        candidates.sort { $0.modified > $1.modified }
        var pending = 0, loaded = 0
        for candidate in candidates {
            if entries[candidate.url]?.signature == candidate.signature { continue }
            if loaded > 0 && Date().timeIntervalSince(started) >= 1.5 { pending += 1; continue }
            loaded += 1
            do {
                try Task.checkCancellation()
                let result: ProviderSessions
                switch source {
                case .antigravity: result = try AntigravitySessions.read(candidate.url)
                case .grok: result = try GrokSessions.read(candidate.url)
                case .cursor: result = ProviderSessions()
                }
                entries[candidate.url] = Entry(signature: candidate.signature, result: result)
            } catch { failed = true }
        }
        if !failed { entries = entries.filter { seen.contains($0.key) } }
        let results = candidates.compactMap { entries[$0.url]?.result }
        var sessions = results.flatMap(\.sessions)
        if source == .grok {
            let unified = Set(sessions.filter { $0.path?.hasSuffix("/unified.jsonl") == true }.map(\.id))
            // The newer inference log owns usage for covered sessions. Keep legacy turn observations and metadata.
            let legacy = Dictionary(sessions.filter { $0.path?.hasSuffix("/updates.jsonl") == true }.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            sessions = sessions.filter { $0.path?.hasSuffix("/unified.jsonl") == true || !unified.contains($0.id) }.map { item in
                var item = item
                if item.path?.hasSuffix("/unified.jsonl") == true, let previous = legacy[item.id] {
                    if !previous.events.isEmpty {
                        overlapNotice = L10n.text("Grok 新旧日志并存：采用新版请求记录，旧历史可能不完整", "Grok log formats overlap: using inference records; older history may be incomplete")
                    }
                    item.title = previous.title; item.workspace = previous.workspace
                    item.turns = previous.turns; item.completions = previous.completions
                    item.startedAt = previous.startedAt
                    item.lastActivity = [item.lastActivity, previous.lastActivity].compactMap { $0 }.max()
                }
                return item
            }
        }
        // Identically named Antigravity SQLite copies in the recognized roots represent the same conversation.
        var byID: [String: ProviderSession] = [:]
        for item in sessions {
            if var previous = byID[item.id] {
                var events = Dictionary(previous.events.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
                for event in item.events {
                    if let old = events[event.id], old != event { failed = true }
                    else { events[event.id] = event }
                }
                previous.events = events.values.sorted { $0.timestamp < $1.timestamp }; byID[item.id] = previous
            } else { byID[item.id] = item }
        }
        let notices = ([failed ? ProviderFailure.local.message : nil, overlapNotice] + results.map(\.notice)).compactMap { $0 }
        return ProviderSessions(sessions: byID.keys.sorted().compactMap { byID[$0] }, notice: notices.isEmpty ? nil : Array(Set(notices)).sorted().joined(separator: " · "),
            indexing: pending > 0 ? IndexProgress(done: candidates.count - pending, total: candidates.count) : nil)
    }
}

enum ProviderFiles {
    static func json(_ url: URL) throws -> ProviderJSON {
        let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        guard size <= 16 * 1024 * 1024 else { throw ProviderFailure.limit }
        return try ProviderJSON.read(Data(contentsOf: url))
    }
    static func lines(_ url: URL, consume: (ProviderJSON, Int) throws -> Void) throws {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var carry = Data(), read = 0, ordinal = 0
        let deadline = Date().addingTimeInterval(3)
        while let chunk = try handle.read(upToCount: 256 * 1024), !chunk.isEmpty {
            try Task.checkCancellation()
            read += chunk.count
            guard read <= 128 * 1024 * 1024, Date() <= deadline else { throw ProviderFailure.limit }
            carry.append(chunk)
            while let newline = carry.firstIndex(of: 10) {
                let line = carry[..<newline]
                ordinal += 1
                if !line.isEmpty { try consume(ProviderJSON.read(Data(line)), ordinal) }
                carry.removeSubrange(...newline)
            }
            guard carry.count <= 16 * 1024 * 1024 else { throw ProviderFailure.limit }
        }
        // Accept a complete last JSON value without a newline; retry a torn tail on the next changed-file scan.
        if !carry.isEmpty, let value = try? ProviderJSON.read(carry) { try consume(value, ordinal + 1) }
    }
}
