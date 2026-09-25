import CoreServices
import Foundation
import os

/// Tells what changed under a set of directories since the previous check, using FSEvents.
/// Directories created later are watched after the next `update()`.
final class FileChangeMonitor {
    /// Shared with the FSEvents callback, which can run while the monitor is being released.
    private final class Changes: Sendable {
        struct State {
            var changed = true
            /// Files reported since the last check; nil when events were dropped or not yet seen, so everything must be scanned.
            var paths: Set<String>? = nil
        }
        static let pathLimit = 10_000
        let state = OSAllocatedUnfairLock(initialState: State())
        /// Called on the monitor's queue after every batch of events, so a waiting reader can wake at once.
        let onChange: (@Sendable () -> Void)?
        init(onChange: (@Sendable () -> Void)?) { self.onChange = onChange }
    }

    private let directories: [String]
    private let changes: Changes
    private let queue = DispatchQueue(label: "app.agenthud.file-changes", qos: .utility)
    private var stream: FSEventStreamRef?
    private var watched: [String]?

    /// - onChange: called after each batch of file events, on the monitor's queue.
    init(directories: [URL], onChange: (@Sendable () -> Void)? = nil) {
        // FSEvents streams are recursive, so a directory inside another one adds nothing.
        let paths = Set(directories.map { $0.standardizedFileURL.path })
        self.directories = paths.filter { path in !paths.contains { $0 != path && path.hasPrefix($0 + "/") } }.sorted()
        changes = Changes(onChange: onChange)
        update()
    }

    /// False when no watched directory exists or the stream could not be created, so sources must be read on a schedule.
    var isWatching: Bool { watched.map { !$0.isEmpty } ?? false }

    deinit { stopStream() }

    /// Watches the directories that exist now; a change in the watched set counts as a change of everything.
    func update() {
        let existing = directories.filter { FileManager.default.fileExists(atPath: $0) }
        guard existing != watched else { return }
        stopStream()
        watched = existing
        changes.state.withLock { $0 = .init() }
        guard !existing.isEmpty else { return }
        var context = FSEventStreamContext(version: 0, info: Unmanaged.passUnretained(changes).toOpaque(), retain: { info in
            guard let info else { return nil }
            _ = Unmanaged<Changes>.fromOpaque(info).retain()
            return info
        }, release: { info in
            guard let info else { return }
            Unmanaged<Changes>.fromOpaque(info).release()
        }, copyDescription: nil)
        let callback: FSEventStreamCallback = { _, info, count, rawPaths, flags, _ in
            guard let info else { return }
            let paths = unsafeBitCast(rawPaths, to: NSArray.self) as? [String] ?? []
            let rescan = FSEventStreamEventFlags(kFSEventStreamEventFlagMustScanSubDirs | kFSEventStreamEventFlagUserDropped
                                                 | kFSEventStreamEventFlagKernelDropped | kFSEventStreamEventFlagRootChanged)
            let dropped = (0..<count).contains { flags[$0] & rescan != 0 } || paths.count != count
            let changes = Unmanaged<Changes>.fromOpaque(info).takeUnretainedValue()
            changes.state.withLock { state in
                state.changed = true
                guard !dropped, var known = state.paths else { state.paths = nil; return }
                known.formUnion(paths)
                state.paths = known.count > Changes.pathLimit ? nil : known
            }
            changes.onChange?()
        }
        let flags = FSEventStreamCreateFlags(kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes)
        guard let created = FSEventStreamCreate(kCFAllocatorDefault, callback, &context, existing as CFArray,
                                                FSEventStreamEventId(kFSEventStreamEventIdSinceNow), 1, flags) else {
            watched = nil
            return
        }
        FSEventStreamSetDispatchQueue(created, queue)
        guard FSEventStreamStart(created) else {
            FSEventStreamInvalidate(created)
            FSEventStreamRelease(created)
            watched = nil
            return
        }
        stream = created
    }

    /// Whether anything changed since the previous call. Without a working stream every call reports a change.
    func consumeChanges() -> Bool {
        let changed = changes.state.withLock { state in
            let changed = state.changed
            state.changed = false
            state.paths = []
            return changed
        }
        return changed || !isWatching
    }

    /// Files reported since the previous call, or nil when a full scan is needed: the first call, dropped events,
    /// a newly watched directory, or no working stream.
    func consumePaths() -> Set<String>? {
        let paths = changes.state.withLock { state in
            let paths = state.paths
            state.changed = false
            state.paths = []
            return paths
        }
        return isWatching ? paths : nil
    }

    private func stopStream() {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        self.stream = nil
    }
}
