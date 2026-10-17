import Foundation

actor OpenAgentLocalStore {
    struct Result: Sendable {
        var sessions: [OpenAgentSession] = []
        var notices: [String: String] = [:]
        var indexing: IndexProgress?
        /// Changes whenever the parsed files change; nil when the reader cannot tell.
        var revision: Int? = nil
        /// The files the sessions come from.
        var files: ListedFiles? = nil
    }
    let paths: OpenAgentPaths
    private var files: WholeFileStore<[OpenAgentSession]>

    init(paths: OpenAgentPaths) {
        self.paths = paths
        func pi(_ data: Data, _ url: URL) throws -> [OpenAgentSession] {
            url.pathExtension == "json" ? [try PiSessionObserver.read(data).session] : try OpenAgentParser.pi(data, path: url.path)
        }
        func listing(_ source: OpenAgentSource, _ roots: [URL], accepts: @escaping (URL) -> Bool, related: @escaping (URL) -> [URL] = { _ in [] },
                     parse: @escaping (Data, URL) throws -> [OpenAgentSession]) -> WholeFileStore<[OpenAgentSession]>.Listing {
            .init(name: source.name, files: LogFiles(roots: roots, watchesChanges: false, limit: 20000, accepts: accepts), related: related, parse: { url, _ in
                let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                guard size <= 64 * 1024 * 1024 else { throw ProviderFailure.limit }
                return try parse(Data(contentsOf: url), url)
            })
        }
        files = WholeFileStore(listings: [
            // SQLite records take precedence over JSON records regardless of file modification time.
            .init(name: OpenAgentSource.opencode.name, files: LogFiles(roots: [paths.openCode.appendingPathComponent("opencode.db")], watchesChanges: false) { _ in true },
                  related: { [URL(fileURLWithPath: $0.path + "-wal")] }, leads: true, parse: { url, since in try OpenAgentParser.openCodeSQLite(url, since: since) }),
            listing(.opencode, [paths.openCode.appendingPathComponent("storage/message")], accepts: { $0.pathExtension == "json" }) { data, url in
                let value = try ProviderJSON.read(data)
                guard let id = value["id"].stringValue, let sid = value["sessionID"].stringValue else { throw ProviderFailure.format }
                return try OpenAgentParser.openCodeMessage(value, id: id, sessionID: sid, path: url.path).map { [$0] } ?? []
            },
            // Completed turns of the OpenCode plugin.
            listing(.opencode, [paths.openCodeTurns], accepts: { $0.pathExtension == "json" }) { data, _ in
                [try OpenCodeSessionObserver.read(data).session]
            },
            // The session's title lives in `state.json`, which a rename changes without touching the wire log.
            listing(.kimi, paths.roots(for: .kimi), accepts: { $0.lastPathComponent == "wire.jsonl" },
                    related: { [OpenAgentParser.kimiSession($0).folder.appendingPathComponent("state.json")] }) { data, url in
                try OpenAgentParser.kimi(data, path: url.path)
            },
            listing(.pi, paths.roots(for: .pi), accepts: { $0.pathExtension == "jsonl" }, parse: pi),
            // Turn observations of the Pi extension, read like transcripts.
            listing(.pi, [paths.piTurns], accepts: { ["jsonl", "json"].contains($0.pathExtension) }, parse: pi),
        ])
    }

    func fileChanges(_ paths: Set<String>?) { files.noteChanges(paths) }

    func index(since: Date) -> Result {
        let pass = files.index(since: since)
        var result = Result(notices: pass.notices)
        var grouped: [String: OpenAgentSession] = [:]
        var workspaceIndexes: [URL: ProviderJSON] = [:]
        for (_, sessions) in pass.files {
            for var item in sessions {
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
                    let newer = (item.end ?? .distantPast) > (prior.end ?? .distantPast)
                    // The observer's copy settles after the log's last reply but may hold an older name.
                    if item.titleSource == prior.titleSource ? newer : item.titleSource > prior.titleSource {
                        prior.title = item.title; prior.titleSource = item.titleSource
                    }
                    if newer {
                        prior.workspace = item.workspace ?? prior.workspace
                        prior.currentModel = item.currentModel ?? prior.currentModel
                        if !item.path.isEmpty { prior.path = item.path }
                    }
                    if let navigation = item.navigation, navigation.observedAt > (prior.navigation?.observedAt ?? .distantPast) {
                        prior.navigation = navigation
                    }
                    prior.events = UsageAggregation.usageUnion([prior.events, item.events])
                    prior.models.merge(item.models, uniquingKeysWith: { old, _ in old })
                    prior.start = [prior.start, item.start].compactMap { $0 }.min()
                    prior.end = [prior.end, item.end].compactMap { $0 }.max()
                    prior.turns = (prior.turns + item.turns).sorted { ($0.startedAtMs ?? $0.observedAtMs) < ($1.startedAtMs ?? $1.observedAtMs) }
                    prior.completions += item.completions
                    if prior.currentModel == nil { prior.currentModel = item.currentModel }
                    if prior.path.isEmpty { prior.path = item.path }
                    // Only the database tells an archived session; its other records never do.
                    prior.archived = prior.archived || item.archived
                    grouped[item.id] = prior
                } else { grouped[item.id] = item }
            }
        }
        result.sessions = grouped.values.sorted { $0.id < $1.id }
        result.indexing = pass.indexing
        result.revision = pass.revision
        result.files = pass.listedFiles { $0.map(\.id) }
        return result
    }
}
