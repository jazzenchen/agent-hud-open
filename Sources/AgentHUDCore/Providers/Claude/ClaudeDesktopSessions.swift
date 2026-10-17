import Foundation

/// Exact CLI-to-tab identifiers from the Claude Desktop metadata files. This index reads no transcript or account files.
public actor ClaudeDesktopSessions {
    private struct Location: Sendable { let url: URL; let cowork: Bool }
    private nonisolated let locations: [Location]
    public nonisolated var roots: [URL] { locations.map(\.url) }
    private static let metadataLimit = 1024 * 1024
    private struct Metadata: Decodable, Sendable {
        let sessionId: String
        let cliSessionId: String
        let isArchived: Bool?
    }
    private struct Stamp: Equatable, Sendable { let modified: Date?; let size: Int }
    private struct Cached: Sendable { let stamp: Stamp; let metadata: Metadata? }
    private var cache: [String: Cached] = [:]

    public init(codeRoot: URL = defaultRoots()[0], coworkRoot: URL? = defaultRoots()[1]) {
        locations = [Location(url: codeRoot, cowork: false)] + (coworkRoot.map { [Location(url: $0, cowork: true)] } ?? [])
    }

    public nonisolated static func defaultRoots(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        let base = home.appendingPathComponent("Library/Application Support/Claude", isDirectory: true)
        return ["claude-code-sessions", "local-agent-mode-sessions"].map { base.appendingPathComponent($0, isDirectory: true) }
    }

    /// Only the requested native session IDs can become destinations. Conflicting active tab mappings are left unresolved.
    public func targets(sessionIDs: Set<String>) -> [String: SessionNavigationTarget] { index(sessionIDs: sessionIDs).targets }

    /// The requested native sessions the desktop app opens in a tab, and those it archived: every tab that holds one is
    /// archived. An archived session has no destination.
    public struct Index: Sendable {
        public var targets: [String: SessionNavigationTarget] = [:]
        public var archived: Set<String> = []
    }

    public func index(sessionIDs: Set<String>) -> Index {
        guard !sessionIDs.isEmpty else { return Index() }
        var files: [String: Cached] = [:], matches: [String: Set<SessionNavigationTarget>] = [:], tabs: [String: (all: Int, archived: Int)] = [:]
        for location in locations {
            for file in Self.metadataFiles(roots: [location.url]) {
                guard let values = try? file.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey]),
                      values.isRegularFile == true, values.isSymbolicLink != true, let size = values.fileSize,
                      size <= Self.metadataLimit else { continue }
                let stamp = Stamp(modified: values.contentModificationDate, size: size)
                let cached: Cached
                if let previous = cache[file.path], previous.stamp == stamp { cached = previous }
                else { cached = Cached(stamp: stamp, metadata: Self.read(file)) }
                files[file.path] = cached
                guard let metadata = cached.metadata, sessionIDs.contains(metadata.cliSessionId),
                      Self.isLocalID(metadata.sessionId), file.deletingPathExtension().lastPathComponent == metadata.sessionId else { continue }
                let archived = metadata.isArchived == true
                tabs[metadata.cliSessionId, default: (0, 0)].all += 1
                if archived { tabs[metadata.cliSessionId, default: (0, 0)].archived += 1; continue }
                let target: SessionNavigationTarget = location.cowork ? .claudeCoworkSession(id: metadata.sessionId) : .claudeDesktopSession(id: metadata.sessionId)
                matches[metadata.cliSessionId, default: []].insert(target)
            }
        }
        cache = files
        var index = Index(archived: Set(tabs.filter { $0.value.archived == $0.value.all }.keys))
        for (id, targets) in matches where targets.count == 1 { index.targets[id] = targets.first }
        return index
    }

    private static func read(_ file: URL) -> Metadata? {
        guard let handle = try? FileHandle(forReadingFrom: file) else { return nil }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: metadataLimit + 1), data.count <= metadataLimit else { return nil }
        // Decode only the identifier and archive fields; titles, prompts, MCP configuration and account details are ignored.
        return try? JSONDecoder().decode(Metadata.self, from: data)
    }

    static func isLocalID(_ id: String) -> Bool { id.hasPrefix("local_") && UUID(uuidString: String(id.dropFirst(6))) != nil }

    /// Walk exactly <root>/<organization>/<account>/local_*.json. Sibling transcript directories are never entered.
    static func metadataFiles(roots: [URL]) -> [URL] {
        func directories(_ root: URL) -> [URL] {
            let files = (try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
                                                                     options: [.skipsHiddenFiles])) ?? []
            return files.filter {
                guard let values = try? $0.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
                return values.isDirectory == true && values.isSymbolicLink != true
            }
        }
        return roots.flatMap { root in
            directories(root).flatMap { organization in
                directories(organization).flatMap { account in
                    ((try? FileManager.default.contentsOfDirectory(at: account, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles])) ?? [])
                        .filter { $0.pathExtension == "json" && $0.lastPathComponent.hasPrefix("local_") }
                }
            }
        }
    }
}
