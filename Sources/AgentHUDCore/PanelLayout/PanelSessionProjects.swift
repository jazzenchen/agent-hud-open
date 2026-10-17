import AgentHUDSupport
import Foundation

extension ReportView {
    /// The projects the view's sessions belong to, account-wide API receipts aside, in the order the statistics
    /// window's Sessions page offers them (`SessionProject.ordered(_:)`): the projects a layout's `sessions.projects`
    /// can list. `project` finds a session's own; a host passes the one it keeps for the report
    /// (`UsageStore.panelSessionProjects`).
    func panelSessionProjects(project: (LiveSession) -> SessionProject = SessionProject.init) -> [SessionProject] {
        SessionProject.ordered(Set(sessions.filter { !$0.session.accountWide }.map { project($0.session) }))
    }
}

/// What a proposed layout's `sessions.projects` name (`PanelLayout.projectMatch(_:among:holding:)`): each entry's
/// project by its id, the entries that name one by a path, and those that name none.
struct PanelProjectMatch: Hashable, Sendable {
    /// An entry given as a path, with the id of the project it names, which the reply tells.
    struct Path: Hashable, Sendable {
        let given: String
        let id: String
    }

    /// Each entry's project, by its id, in the order given; an entry that names none is left out, as is one that is no
    /// string or is empty, which the reading tells (`PanelLayout.misreadings(of:)`).
    let ids: [String]
    /// The entries that name a project other than by its id, in the order given.
    let paths: [Path]
    /// Every entry that names no project, by its place in the list.
    let issues: [PanelLayoutIssue]
}

extension PanelLayout {
    /// What the entries of a proposal's `sessions.projects`, as sent, name among the projects there are: `offered`, those
    /// the sessions belong to now (`ReportView.panelSessionProjects(project:)`), and those the `held` layouts list, which
    /// a layout keeps for when their sessions return, so a proposal built on a layout that was read may carry them. An
    /// entry names the project whose id it is; else, where it is a path, absolute or written with ~, the innermost of
    /// those projects it lies in, as it is written, with its links resolved, or as Git places it
    /// (`SessionProject.directory(for:)`), where a worktree is its repository's checkout: so an agent names the project
    /// it works in by its own working directory. An entry that names none is an issue, and so is one that names a
    /// project by its name in place of its id, since two projects can share a name.
    static func projectMatch(_ value: JSONValue, among offered: [SessionProject],
                             holding held: [PanelLayout] = []) -> PanelProjectMatch {
        let kept = held.flatMap(\.sessions.projects).filter { !$0.isEmpty }.map(SessionProject.init(id:))
        let candidates = (offered + kept).withoutRepeats
        let listed = offered.isEmpty ? "No project has a session now."
            : "Projects with sessions now: \(quoted(offered.map(\.id)))."
        var ids: [String] = [], paths: [PanelProjectMatch.Path] = [], issues: [PanelLayoutIssue] = []
        for (index, item) in (value.arrayValue ?? []).enumerated() {
            guard let entry = item.stringValue, !entry.isEmpty else { continue }
            if let project = project(named: entry, among: candidates) {
                ids.append(project.id)
                if project.id != entry { paths.append(PanelProjectMatch.Path(given: entry, id: project.id)) }
                continue
            }
            let named = offered.filter { $0.path != nil && $0.title == entry }
            let message = named.isEmpty
                ? "\"\(entry)\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. \(listed)"
                : "\"\(entry)\" is a project's name, not its id. Name it by its id: \(quoted(named.map(\.id)))."
            issues.append(PanelLayoutIssue(path: "sessions.projects[\(index)]", message: message))
        }
        return PanelProjectMatch(ids: ids, paths: paths, issues: issues)
    }

    /// The project of `candidates` an entry names: the one whose id it is; else, for a path, the innermost one whose
    /// folder it is or lies in, by any of its spellings: as written, with its links resolved, or as Git places it.
    private static func project(named entry: String, among candidates: [SessionProject]) -> SessionProject? {
        if let project = candidates.first(where: { $0.id == entry }) { return project }
        let expanded = (entry as NSString).expandingTildeInPath
        guard expanded.hasPrefix("/") else { return nil }
        let written = URL(fileURLWithPath: expanded).standardizedFileURL.path
        let spellings = [written, URL(fileURLWithPath: written).resolvingSymlinksInPath().path,
                         SessionProject.directory(for: written)].withoutRepeats
        var innermost: (project: SessionProject, length: Int)?
        for project in candidates {
            guard let path = project.path else { continue }
            let folder = URL(fileURLWithPath: path).standardizedFileURL.path
            let inside = folder == "/" ? "/" : folder + "/"
            guard spellings.contains(where: { $0 == folder || $0.hasPrefix(inside) }) else { continue }
            if folder.count > innermost?.length ?? -1 { innermost = (project, folder.count) }
        }
        return innermost?.project
    }
}
