import Foundation

actor AdditionalLocalStore {
    let source: AdditionalSource
    let roots: [URL]
    private var files: WholeFileStore<ProviderSessions>

    init(source: AdditionalSource, roots: [URL]? = nil) {
        self.source = source
        let layout = source.layout
        let roots = roots ?? layout?.roots(home: FileManager.default.homeDirectoryForCurrentUser, environment: ProcessInfo.processInfo.environment) ?? []
        self.roots = roots
        files = WholeFileStore(listings: layout.map { layout in
            [.init(name: source.vendor, files: LogFiles(roots: roots, watchesChanges: false, limit: 20000, skips: { layout.skips($0) }, accepts: { layout.accepts($0) }),
                   related: { layout.related($0) }, parse: { url, _ in try layout.read(url) })]
        } ?? [])
    }

    func fileChanges(_ paths: Set<String>?) { files.noteChanges(paths) }

    func index(since: Date) -> ProviderSessions {
        let pass = files.index(since: since)
        var failed = !pass.notices.isEmpty
        var sessions = pass.files.flatMap(\.parsed.sessions)
        let mergeNotice = source.layout?.notice(merging: sessions)
        if let layout = source.layout { sessions = layout.merge(sessions) }
        // Identically named Antigravity SQLite copies in the recognized roots represent the same conversation.
        var byID: [String: ProviderSession] = [:]
        for item in sessions {
            if var previous = byID[item.id] {
                var events = Dictionary(previous.events.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
                for event in item.events {
                    if let old = events[event.id], old != event { failed = true }
                    else { events[event.id] = event }
                }
                previous.events = events.values.sorted { $0.timestamp < $1.timestamp }
                previous.archived = previous.archived && item.archived
                byID[item.id] = previous
            } else { byID[item.id] = item }
        }
        let notices = ([failed ? ProviderFailure.local.message : nil, mergeNotice] + pass.files.map(\.parsed.notice)).compactMap { $0 }
        return ProviderSessions(sessions: byID.keys.sorted().compactMap { byID[$0] }, notice: notices.isEmpty ? nil : Array(Set(notices)).sorted().joined(separator: " · "),
            indexing: pass.indexing, revision: pass.revision, files: pass.listedFiles { $0.sessions.map(\.id) },
            // Sessions the layout merged across files keep every file's models for them.
            models: pass.files.reduce(into: [:]) { $0.merge($1.parsed.models) { $0.union($1) } },
            apiReached: pass.files.reduce(into: Set<String>()) { $0.formUnion($1.parsed.apiReached) },
            limitHits: pass.files.flatMap(\.parsed.limitHits))
    }
}
