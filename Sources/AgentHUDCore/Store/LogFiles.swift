import Foundation

/// The log files under a set of roots with their modification times and sizes. With a directory watch, a poll looks
/// only at the files reported changed; without one, or every five minutes and whenever events were dropped, it lists
/// every file again.
final class LogFiles {
    struct File: Equatable {
        let modified: Date
        let size: Int
    }

    /// What a full listing could not cover, for a notice: entries it could not read, and whether it stopped at its limit.
    struct Gaps {
        var unreadable = 0
        var truncated = false
    }

    static let fullScanInterval: TimeInterval = 300

    private let roots: [URL]
    private let limit: Int
    private let skips: (URL) -> Bool
    private let accepts: (URL) -> Bool
    private var monitor: FileChangeMonitor?
    private(set) var files: [String: File] = [:]
    private var scannedAt: Date?
    /// The outer collector's event paths. Once it supplies them, it owns the watch for this listing; its paths must
    /// reach the index even when this listing's independent FSEvents stream has not delivered the same event yet.
    private var usesExternalChanges = false
    private var reportedPaths: Set<String>? = []

    /// - limit: entries a full listing visits before it stops.
    /// - skips: an entry the listing neither accepts nor descends into.
    init(roots: [URL], watchesChanges: Bool, limit: Int = .max, skips: @escaping (URL) -> Bool = { _ in false },
         accepts: @escaping (URL) -> Bool) {
        self.roots = roots
        self.limit = limit
        self.skips = skips
        self.accepts = accepts
        monitor = watchesChanges ? FileChangeMonitor(directories: roots) : nil
    }

    /// Called before a collection read with paths from the collector's watch. Nil means events were dropped and a full
    /// listing is required. Repeated signals accumulate until the next read.
    func noteChanges(_ paths: Set<String>?) {
        usesExternalChanges = true
        monitor = nil
        if let paths, reportedPaths != nil { reportedPaths?.formUnion(paths.filter { canonical($0) != nil }) }
        else { reportedPaths = nil }
    }

    /// Brings `files` up to date.
    @discardableResult
    func refresh(now: Date) -> Gaps {
        let changed: Set<String>?
        if usesExternalChanges {
            changed = reportedPaths
            reportedPaths = []
        } else {
            monitor?.update()
            changed = monitor?.consumePaths()
        }
        if let changed, let scannedAt, now.timeIntervalSince(scannedAt) < Self.fullScanInterval {
            // FSEvents can name a directory when it is created or removed. Its children cannot be inferred from that
            // one path; enumerate once so new rollouts and deleted subtrees are both reflected.
            if changed.contains(where: { path in
                guard let path = canonical(path) else { return false }
                var directory: ObjCBool = false
                return (FileManager.default.fileExists(atPath: path, isDirectory: &directory) && directory.boolValue)
                    || files.keys.contains(where: { $0.hasPrefix(path + "/") })
            }) { return scan(now: now) }
            for path in changed.compactMap(canonical) {
                let url = URL(fileURLWithPath: path)
                guard accepts(url) else { continue }
                if let values = try? url.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]),
                   values.isRegularFile == true, let modified = values.contentModificationDate, let size = values.fileSize {
                    files[path] = File(modified: modified, size: size)
                } else if !FileManager.default.fileExists(atPath: path) {
                    files.removeValue(forKey: path)
                }
            }
            return Gaps()
        }
        return scan(now: now)
    }

    private func scan(now: Date) -> Gaps {
        let keys: [URLResourceKey] = [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]
        var listed: [String: File] = [:], gaps = Gaps(), visited = 0
        func list(_ url: URL) {
            guard let values = try? url.resourceValues(forKeys: Set(keys)), let modified = values.contentModificationDate,
                  let size = values.fileSize, values.isRegularFile != nil else { gaps.unreadable += 1; return }
            if values.isRegularFile == true { listed[url.path] = File(modified: modified, size: size) }
        }
        roots: for root in roots {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDirectory) else { continue }
            guard isDirectory.boolValue else {
                // A root can name one file; a new URL does not answer from values cached by an earlier listing.
                let url = URL(fileURLWithPath: root.path)
                if accepts(url) { list(url) }
                continue
            }
            guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles], errorHandler: { _, _ in
                gaps.unreadable += 1
                return true
            }) else {
                gaps.unreadable += 1
                continue
            }
            for case let url as URL in enumerator {
                visited += 1
                guard visited <= limit else { gaps.truncated = true; break roots }
                if skips(url) { enumerator.skipDescendants(); continue }
                if accepts(url) { list(url) }
            }
        }
        files = listed
        scannedAt = now
        return gaps
    }

    /// Change events can name a root by its resolved path; files are keyed as the listing names them.
    private func canonical(_ path: String) -> String? {
        for root in roots {
            if path == root.path || path.hasPrefix(root.path + "/") { return path }
            let resolved = root.resolvingSymlinksInPath().path
            if resolved != root.path, path == resolved || path.hasPrefix(resolved + "/") {
                return root.path + path.dropFirst(resolved.count)
            }
        }
        return nil
    }
}
