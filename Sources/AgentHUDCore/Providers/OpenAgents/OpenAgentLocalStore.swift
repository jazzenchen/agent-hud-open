import Foundation

actor OpenAgentLocalStore {
    struct Result: Sendable {
        var sessions: [OpenAgentSession] = []
        var notices: [String: String] = [:]
        var indexing: IndexProgress?
    }
    let paths: OpenAgentPaths
    private struct Entry { let signature: String; let sessions: [OpenAgentSession] }
    private var cache: [URL: Entry] = [:]
    init(paths: OpenAgentPaths) { self.paths = paths }

    func index(since: Date) -> Result {
        let manager = FileManager.default, started = Date()
        var result = Result(), candidates: [(URL, OpenAgentSource, String, Date)] = []
        var seen = Set<URL>()
        func candidate(_ url: URL, source: OpenAgentSource) {
            guard let attributes = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey]),
                  attributes.isRegularFile == true, let modified = attributes.contentModificationDate else { return }
            var date = modified, signature = "\(modified.timeIntervalSince1970):\(attributes.fileSize ?? 0)"
            if url.pathExtension == "db", let wal = try? URL(fileURLWithPath: url.path + "-wal").resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey]), let updated = wal.contentModificationDate {
                date = max(date, updated); signature += ":\(updated.timeIntervalSince1970):\(wal.fileSize ?? 0)"
            }
            guard date >= since, seen.insert(url).inserted else { return }
            candidates.append((url, source, signature, date))
        }
        for source in [OpenAgentSource.opencode, .kimi, .pi] {
            let roots = source == .opencode ? [paths.openCode.appendingPathComponent("storage/message")]
                : paths.roots(for: source) + (source == .pi ? [paths.piTurns] : [])
            if source == .opencode { candidate(paths.openCode.appendingPathComponent("opencode.db"), source: source) }
            var visited = 0
            for root in roots where manager.fileExists(atPath: root.path) {
                guard let enumerator = manager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey], options: [.skipsHiddenFiles], errorHandler: { _, _ in
                    result.notices[source.name] = ProviderFailure.local.message; return false
                }) else { result.notices[source.name] = ProviderFailure.local.message; continue }
                for case let url as URL in enumerator {
                    visited += 1
                    if visited > 20000 { result.notices[source.name] = ProviderFailure.limit.message; break }
                    let accepted = source == .opencode ? url.pathExtension == "json" : source == .kimi ? url.lastPathComponent == "wire.jsonl"
                        : url.pathExtension == "jsonl" || (root == paths.piTurns && url.pathExtension == "json")
                    if accepted { candidate(url, source: source) }
                }
            }
        }
        // SQLite records take precedence over JSON records regardless of file modification time.
        candidates.sort { a, b in
            if (a.0.pathExtension == "db") != (b.0.pathExtension == "db") { return a.0.pathExtension == "db" }
            return a.3 > b.3
        }
        var pending = 0, loaded = 0
        for (url, source, signature, _) in candidates {
            if cache[url]?.signature == signature { continue }
            if loaded > 0 && Date().timeIntervalSince(started) >= 1.5 { pending += 1; continue }
            loaded += 1
            do {
                try Task.checkCancellation()
                let sessions: [OpenAgentSession]
                if url.pathExtension == "db" { sessions = try OpenAgentParser.openCodeSQLite(url, since: since) }
                else {
                    let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                    guard size <= 64 * 1024 * 1024 else { throw ProviderFailure.limit }
                    let data = try Data(contentsOf: url)
                    switch source {
                    case .pi:
                        sessions = url.pathExtension == "json" ? [try PiSessionObserver.read(data).session]
                            : try OpenAgentParser.pi(data, path: url.path)
                    case .kimi: sessions = try OpenAgentParser.kimi(data, path: url.path)
                    case .opencode:
                        let value = try ProviderJSON.read(data)
                        guard let id = value["id"].stringValue, let sid = value["sessionID"].stringValue else { throw ProviderFailure.format }
                        sessions = try OpenAgentParser.openCodeMessage(value, id: id, sessionID: sid, path: url.path).map { [$0] } ?? []
                    case .glm: sessions = []
                    }
                }
                cache[url] = .init(signature: signature, sessions: sessions)
            } catch { result.notices[source.name] = ProviderFailure.local.message }
        }
        if result.notices.isEmpty { cache = cache.filter { seen.contains($0.key) } }
        var grouped: [String: OpenAgentSession] = [:]
        var workspaceIndexes: [URL: ProviderJSON] = [:]
        for (url, _, _, _) in candidates {
            for var item in cache[url]?.sessions ?? [] {
                if item.client == .kimi {
                    let file = URL(fileURLWithPath: item.path)
                    let agent = file.deletingLastPathComponent()
                    if agent.deletingLastPathComponent().lastPathComponent == "agents" {
                        let workspace = agent.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
                        let root = workspace.deletingLastPathComponent().deletingLastPathComponent()
                        if workspaceIndexes[root] == nil { workspaceIndexes[root] = OpenAgentCredentials.read(root.appendingPathComponent("workspaces.json"))["workspaces"] }
                        let metadata = workspaceIndexes[root]?[workspace.lastPathComponent] ?? .null
                        item.workspace = metadata["root"].stringValue
                    }
                }
                if var prior = grouped[item.id] {
                    if (item.end ?? .distantPast) > (prior.end ?? .distantPast) {
                        prior.title = item.title; prior.workspace = item.workspace ?? prior.workspace
                        prior.currentModel = item.currentModel ?? prior.currentModel
                        if !item.path.isEmpty { prior.path = item.path }
                    }
                    prior.events = UsageAggregation.usageUnion([prior.events, item.events])
                    prior.models.merge(item.models, uniquingKeysWith: { old, _ in old })
                    prior.start = [prior.start, item.start].compactMap { $0 }.min()
                    prior.end = [prior.end, item.end].compactMap { $0 }.max()
                    prior.turns = (prior.turns + item.turns).sorted { ($0.startedAtMs ?? $0.observedAtMs) < ($1.startedAtMs ?? $1.observedAtMs) }
                    prior.completions += item.completions
                    if prior.currentModel == nil { prior.currentModel = item.currentModel }
                    if prior.path.isEmpty { prior.path = item.path }
                    grouped[item.id] = prior
                } else { grouped[item.id] = item }
            }
        }
        result.sessions = grouped.values.sorted { $0.id < $1.id }
        result.indexing = pending > 0 ? IndexProgress(done: candidates.count - pending, total: candidates.count) : nil
        return result
    }
}
