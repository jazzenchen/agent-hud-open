import Foundation

/// A Git repository, shared by its worktrees, or the recorded directory when it is outside Git.
public enum SessionProject: Hashable, Sendable {
    case directory(String)
    case unassigned

    public init(_ session: LiveSession) {
        if let path = session.workingDirectory, !path.isEmpty { self = .directory(Self.directory(for: path)) }
        else { self = .unassigned }
    }

    public var path: String? {
        if case .directory(let path) = self { return path }
        return nil
    }

    public var title: String {
        path.map { ($0 as NSString).lastPathComponent } ?? L10n.text("未归属项目", "Unassigned project")
    }

    public var label: String {
        path.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? L10n.text("未归属项目", "Unassigned project")
    }

    /// The id of the sessions of no folder, which no folder written with ~ or from the root is.
    static let unassignedID = "unassigned"

    /// How the hover panel's layout names the project (`PanelLayout.Sessions.projects`) and `get_hud` gives it: its
    /// folder written with ~, as `label` writes it, the same in every language; `unassigned` for the sessions of none.
    public var id: String {
        path.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? Self.unassignedID
    }

    /// The project an id names: the folder it writes, with ~ expanded, or the sessions of none for `unassigned`.
    public init(id: String) {
        self = id == Self.unassignedID ? .unassigned : .directory((id as NSString).expandingTildeInPath)
    }

    /// Projects in the order the statistics window's Sessions page and the hover panel's layout offer them: by label,
    /// as Finder orders names, and the sessions of no folder last.
    public static func ordered(_ projects: some Sequence<SessionProject>) -> [SessionProject] {
        projects.sorted {
            if $0 == .unassigned { return false }
            if $1 == .unassigned { return true }
            return $0.label.localizedStandardCompare($1.label) == .orderedAscending
        }
    }

    /// Read Git's own pointers instead of inferring a repository from a folder name or a worktree location.
    static func directory(for path: String) -> String {
        var directory = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.resolvingSymlinksInPath()
        while true {
            let marker = directory.appendingPathComponent(".git")
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: marker.path, isDirectory: &isDirectory) {
                if isDirectory.boolValue { return directory.path }
                guard let pointer = try? String(contentsOf: marker, encoding: .utf8), pointer.hasPrefix("gitdir:") else { return path }
                guard let gitDirectory = resolve(String(pointer.dropFirst("gitdir:".count)), relativeTo: directory) else { return path }
                // Submodules and separate Git directories have no commondir: their checkout stays its own project.
                guard let common = try? String(contentsOf: gitDirectory.appendingPathComponent("commondir"), encoding: .utf8) else {
                    return directory.path
                }
                guard let commonDirectory = resolve(common, relativeTo: gitDirectory) else { return path }
                return commonDirectory.lastPathComponent == ".git" ? commonDirectory.deletingLastPathComponent().path : commonDirectory.path
            }
            // Legacy Foundation appends ".." when deleting the root's last component.
            if directory.path == "/" { return path }
            let parent = directory.deletingLastPathComponent()
            if parent.path == directory.path { return path }
            directory = parent
        }
    }

    private static func resolve(_ pointer: String, relativeTo directory: URL) -> URL? {
        let path = pointer.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true, relativeTo: directory)
            .standardizedFileURL.resolvingSymlinksInPath()
    }
}
