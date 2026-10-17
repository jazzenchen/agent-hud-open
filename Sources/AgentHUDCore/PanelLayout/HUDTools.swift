import AgentHUDSupport
import Foundation

/// What Agent HUD's MCP tools do in the running application: read what the hover panel shows, draw a picture of it, of
/// one agent's own panel or of a screen's resting HUD, propose a layout for it, and drop a proposal, for the shared
/// layout or for one screen's own, whose proposal can also change the style of that screen's resting HUD, or change
/// that alone. Any proposal can also change the app's own settings the person asked for (`AppDraft`), or change those
/// alone. An edit of a layout, or a drop, holds a screen's panel open while the agent works (`PanelHolds`), until the
/// agent says it is done.
///
/// Every answer comes from what the application holds: the last report, the settings and the layout store, the
/// attached screens, and the panel's own views, which the host measures and draws off screen through the closures it
/// passes in. Nothing here starts a provider read, opens the panel itself or writes the agent list or the settings: a
/// hold is a signal the host's panel follows, without a read. Nothing here saves: a proposal is a draft, a screen's
/// style and the app's own settings drawn over the settings, until a person presses Save in the panel. A proposal is
/// checked as an agent sent it, and one the panel or the HUD cannot do as asked is refused whole, with every reason, so
/// an agent never believes a change that fell back. What the layout already holds for a block, a row or an agent that is away may come back with
/// it, so the layout that was read can be sent again, and a proposal is kept as what it changes, so one that draws what
/// is saved leaves no draft. Screens are named by role, agents by vendor and projects by their folder, and a block's
/// accounts by their id, with the email or name the panel writes, since the app and the agent it answers are the
/// person's own.
@MainActor
public struct HUDTools {
    let store: UsageStore
    /// The number the revisions an agent is given are counted from.
    let launch: Int64
    /// Lists the attached screens; nil where the host has none to tell.
    let screens: (@MainActor () -> [PanelScreen])?
    /// Draws a scope's panel as `get_hud_screenshot` returns it; nil where nothing can draw it.
    let picture: (@MainActor (PanelLayoutScope) -> PanelPicture?)?
    /// Measures a scope's panel without drawing it; nil where nothing can.
    let fit: (@MainActor (PanelLayoutScope) -> PanelFit?)?
    /// Draws an attached screen's resting HUD, by its key, as `get_hud_screenshot` returns it with show "hud"; nil
    /// where nothing can draw it.
    let hud: (@MainActor (String) -> HUDPicture?)?
    /// Draws one agent's own panel on an attached screen whose dock separates its agents, by the screen's key and the
    /// agent's vendor as that screen lists it, as `get_hud_screenshot` returns it with an agent; nil where nothing can
    /// draw it.
    let agentPicture: (@MainActor (String, String) -> PanelPicture?)?

    /// Drawn once a launch. The layout store counts its changes from 0 at every launch, so counted from 0 a revision
    /// an agent read before a relaunch would pass for the current one after it.
    private static let drawnAtLaunch = Int64.random(in: 1...999_999) * 1_000

    /// - screens: the attached displays, in the system's order, as `get_hud` lists them; without it no screen is
    ///   listed and only the shared layout, or a screen the store already keeps a layout for, can be addressed.
    /// - picture: draws the hover panel of a scope off screen, as the layout in effect there draws it, at that screen's
    ///   width and insets (the main display's for the shared layout), with account names masked, session titles left
    ///   out and what providers and clients wrote as placeholder bars; without it `get_hud_screenshot` is refused.
    /// - fit: measures that panel off screen; without it `get_hud` leaves out the panel's height and whether it
    ///   scrolls.
    /// - hud: draws an attached screen's resting HUD off screen as it looks now, a style draft included, without
    ///   opening anything; without it `get_hud_screenshot` refuses show "hud".
    /// - agentPicture: draws one agent's own panel off screen, the card a click on its mark opens beside it on a dock
    ///   that separates its agents, as the layout in effect on that screen draws it, at the screen's `agentPanelWidth`,
    ///   masked as `picture` masks the whole panel; without it `get_hud_screenshot` refuses an agent.
    public init(store: UsageStore, screens: (@MainActor () -> [PanelScreen])? = nil,
                picture: (@MainActor (PanelLayoutScope) -> PanelPicture?)? = nil,
                fit: (@MainActor (PanelLayoutScope) -> PanelFit?)? = nil,
                hud: (@MainActor (String) -> HUDPicture?)? = nil,
                agentPicture: (@MainActor (String, String) -> PanelPicture?)? = nil) {
        self.init(store: store, launch: Self.drawnAtLaunch, screens: screens, picture: picture, fit: fit, hud: hud,
                  agentPicture: agentPicture)
    }

    /// - launch: the number revisions are counted from, where a test names its own.
    init(store: UsageStore, launch: Int64, screens: (@MainActor () -> [PanelScreen])? = nil,
         picture: (@MainActor (PanelLayoutScope) -> PanelPicture?)? = nil,
         fit: (@MainActor (PanelLayoutScope) -> PanelFit?)? = nil,
         hud: (@MainActor (String) -> HUDPicture?)? = nil,
         agentPicture: (@MainActor (String, String) -> PanelPicture?)? = nil) {
        self.store = store
        self.launch = launch
        self.screens = screens
        self.picture = picture
        self.fit = fit
        self.hud = hud
        self.agentPicture = agentPicture
    }

    var layout: PanelLayoutStore { store.settings.panelLayout }

    /// The layouts' revision as an agent is given it: one number for every document.
    var revision: Int64 { launch + Int64(layout.revision) }

    public func answer(_ request: MCPToolRequest) -> MCPToolReply {
        let attached = screens?() ?? []
        let arguments = request.arguments
        switch request.tool {
        case MCPTools.getHUD:
            if let unknown = Self.unknown(arguments, taking: ["screen"]) {
                return .failure("get_hud takes only screen. \(unknown)")
            }
            do { return .result(panel(try scope(of: arguments, among: attached), among: attached)) }
            catch { return .failure(Self.sentence(error)) }
        case MCPTools.getHUDScreenshot:
            if let unknown = Self.unknown(arguments, taking: ["screen", "show", "agent"]) {
                return .failure("get_hud_screenshot takes only screen, show and agent. \(unknown)")
            }
            do {
                let scope = try scope(of: arguments, among: attached)
                // An agent given as null is not given, as any key is.
                let agent = arguments["agent"] ?? .null
                switch arguments["show"] ?? .null {
                case .null, .string("panel"):
                    return agent == .null ? image(scope, among: attached) : agentImage(scope, agent: agent, among: attached)
                case .string("hud"):
                    guard agent == .null else {
                        return .failure("show \"hud\" draws a screen's resting HUD, and agent one agent's own panel, so "
                                        + "they are not given together: leave out show, or agent.")
                    }
                    return hudImage(scope, among: attached)
                default: return .failure("show is \"panel\", the default, or \"hud\".")
                }
            } catch {
                return .failure(Self.sentence(error))
            }
        case MCPTools.draftHUD:
            return propose(arguments, among: attached)
        case MCPTools.discardHUDDraft:
            if let unknown = Self.unknown(arguments, taking: ["screen"]) {
                return .failure("Nothing was changed. discard_hud_draft takes only screen. \(unknown)")
            }
            do {
                let scope = try scope(of: arguments, among: attached)
                let styled = layout.hasStyleDraft(scope.screenKey), before = effective(scope)
                // Dropping a screen's style draft alone restyles its resting HUD, which the panel would cover.
                let styleAlone = styled && scope.screenKey.flatMap(layout.draft(of:)) == nil
                layout.discard(scope)
                if styleAlone, let key = scope.screenKey {
                    holds.pause(key)
                } else {
                    holdForEdit(scope, before: before, among: attached)
                }
                var note = discardNote(scope, among: attached)
                if styled { note += " Its resting HUD is back to the style it has saved." }
                // The app's settings are no layout's, and change only when the person asks for one: a drop leaves them.
                if layout.appDraft != nil {
                    note += " The app's own settings keep their unsaved draft, which only draft_hud's app changes: "
                        + "an app that sets nothing drops it."
                }
                return .result(panel(scope, among: attached, note: note))
            } catch {
                return .failure("Nothing was changed. " + Self.sentence(error))
            }
        case MCPTools.finishHUDEdit:
            if let unknown = Self.unknown(arguments, taking: ["screen"]) {
                return .failure("Nothing was changed. finish_hud_edit takes only screen. \(unknown)")
            }
            do { return .result(finish(try scope(of: arguments, among: attached), among: attached)) } catch {
                return .failure("Nothing was changed. " + Self.sentence(error))
            }
        default:
            return .failure("Agent HUD has no tool named \"\(request.tool)\".")
        }
    }

    /// Names the arguments a tool was sent and does not take; nil when it takes them all. An argument a tool does not
    /// take is something its caller believes was heeded, so it is refused, not passed over.
    private static func unknown(_ arguments: [String: JSONValue], taking taken: [String]) -> String? {
        let unknown = arguments.keys.filter { !taken.contains($0) }.sorted()
        return unknown.isEmpty ? nil : "Unknown: \(PanelLayout.quoted(unknown))."
    }

    // MARK: Screens

    /// Why a call cannot be answered as asked, as a sentence for the agent.
    private struct Refusal: Error { let sentence: String }

    private static func sentence(_ error: Error) -> String { (error as? Refusal)?.sentence ?? "The call could not be read." }

    /// The document a call's `screen` names: the shared layout for "all" or none, else that screen's own. A key must
    /// name an attached screen or one the store keeps a layout for, which may be away.
    private func scope(of arguments: [String: JSONValue], among attached: [PanelScreen]) throws -> PanelLayoutScope {
        guard let given = arguments["screen"], given != .null else { return .shared }
        guard let key = given.stringValue else {
            throw Refusal(sentence: "screen is a screen key from get_hud's screens, or \"all\" for the shared layout.")
        }
        if key == "all" { return .shared }
        guard attached.contains(where: { $0.key == key }) || layout.screenKeys.contains(key) else {
            throw Refusal(sentence: "No screen has the key \"\(key)\". \(Self.attachedList(attached)) "
                          + "Use \"all\" for the shared layout.")
        }
        return .screen(key)
    }

    static func attachedList(_ attached: [PanelScreen]) -> String {
        guard !attached.isEmpty else { return "No screen is attached now." }
        return "The screens attached now: " + attached.map { "\"\($0.key)\" (\($0.name))" }.joined(separator: ", ") + "."
    }

    /// A screen as a sentence names it: its role in quotes, or its key where it is not attached now.
    /// - starting: whether the name starts the sentence.
    func name(of key: String, among attached: [PanelScreen], starting: Bool = false) -> String {
        if let screen = attached.first(where: { $0.key == key }) { return "\"\(screen.name)\"" }
        return (starting ? "The" : "the") + " screen \"\(key)\", which is not attached now,"
    }

    // MARK: Proposing

    private static let layoutKeys = ["blocks", "order", "sections", "hiddenSections", "countdown", "style", "hud", "sessions",
                                     "tokens"]
    private static let draftNote = "Shown in the panel as an unsaved draft. The user keeps it by hovering the Agent HUD and "
        + "pressing Save in the panel; it is dropped when Agent HUD restarts."
    private static let savedNote = "This is the layout already saved, so there is no draft."

    private func propose(_ arguments: [String: JSONValue], among attached: [PanelScreen]) -> MCPToolReply {
        let taken = Self.layoutKeys + ["app", "from", "revision", "screen"]
        if let unknown = Self.unknown(arguments, taking: taken) {
            return .failure("Nothing was changed. \(unknown) It takes: \(PanelLayout.quoted(taken)).")
        }
        let scope: PanelLayoutScope
        do { scope = try self.scope(of: arguments, among: attached) } catch {
            return .failure("Nothing was changed. " + Self.sentence(error))
        }
        if let given = arguments["revision"], given != .null {
            guard case .integer(let read) = given else {
                return .failure("Nothing was changed. revision is the whole number get_hud returned.")
            }
            guard read == revision else {
                return .failure("Nothing was changed. The layout changed since you read it: its revision is now "
                                + "\(revision). Call get_hud again, then propose.")
            }
        }
        // What the panel drew before, against which the edit's first agent is found.
        let before = effective(scope)
        // The app's settings are every screen's, whatever screen the call names, and a call that names none leaves them.
        let app = AppProposal(app: arguments["app"], tools: self)
        if let from = arguments["from"], from != .null {
            // A key given as null is not given, here as everywhere.
            guard Self.layoutKeys.allSatisfy({ (arguments[$0] ?? .null) == .null }) else {
                return .failure("Nothing was changed. Pass either from or a layout, not both.")
            }
            guard app.reasons.isEmpty else { return .failure("Nothing was changed. " + app.reasons.joined(separator: " ")) }
            // For a screen, the previous version takes its resting HUD's style with its layout, as a person's Revert to
            // Previous does; the default layout and using the shared layout leave the style as saved, as the layout's
            // Reset to Default and Use Shared Layout do, and hud.reset proposes the default style. A proposal is whole,
            // so a style draft it does not bring is dropped.
            let target = attached.first { $0.key == scope.screenKey }
            var away = false
            switch from.stringValue {
            case "default":
                layout.proposeDefault(scope)
                if let key = scope.screenKey { layout.discardStyle(key) }
            case "previous":
                guard layout.canRevert(scope) else {
                    if case .screen(let key) = scope {
                        let name = name(of: key, among: attached, starting: true)
                        return .failure("Nothing was changed. \(name) has no previous saved layout.")
                    }
                    return .failure("Nothing was changed. There is no previous saved layout.")
                }
                away = scope.screenKey.map { target == nil && layout.previousChangesLook(of: $0) } ?? false
                layout.proposePrevious(scope, hasNotch: target?.hasNotch)
            case "shared":
                guard case .screen(let key) = scope else {
                    return .failure("Nothing was changed. from \"shared\" is for a screen: it proposes that the screen "
                                    + "use the shared layout, which is screen \"all\".")
                }
                layout.propose(.shared, for: key)
                layout.discardStyle(key)
            default:
                return .failure("Nothing was changed. from is \"default\", \"previous\" or \"shared\".")
            }
            proposeApp(app)
            holdForEdit(scope, before: before, among: attached)
            var note = proposalNote(scope, among: attached)
            if away {
                note += " The placement and glow of its resting HUD in that version are not part of the draft while the "
                    + "screen is not attached."
            }
            if app.given { note += " " + appNote() }
            return .result(panel(scope, among: attached, note: note, proposed: true))
        }
        let document = JSONValue.object(arguments.filter { Self.layoutKeys.contains($0.key) })
        let view = store.view, existing = view.panelBlocks
        let saved = savedLayout(scope)
        let held = [effective(scope), saved]
        // Every reason at once: what the reading would forgive, then what the rest of the layout asks that the panel
        // or the screen's resting HUD cannot do. What the layout already holds for a block, a row or an agent that is
        // away may come back with it.
        var reasons = PanelLayout.misreadings(of: document)
        var proposal = PanelLayout(json: document) ?? PanelLayout()
        // A project named by a path inside it, an agent's own working directory among them, is listed by its id.
        let projects = PanelLayout.projectMatch(document["sessions"]["projects"], among: store.panelSessionProjects,
                                                holding: held)
        proposal.sessions.projects = projects.ids
        let style = StyleProposal(hud: arguments["hud"]?.objectValue ?? [:], scope: scope, among: attached, tools: self)
        reasons += proposal.issues(against: existing, holding: held).map { "\($0.path): \($0.message)" }
        reasons += proposal.filterIssues(sessionAgents: view.panelSessionAgents, tokenAgents: view.panelTokenAgents,
                                         tokenModels: view.panelTokenModels, holding: held).map { "\($0.path): \($0.message)" }
        reasons += projects.issues.map { "\($0.path): \($0.message)" }
        reasons += style.reasons
        reasons += proposal.hudIssues(against: agents(for: scope, among: attached, in: style.mode),
                                      whose: whose(scope, among: attached), holding: held).map { "\($0.path): \($0.message)" }
        reasons += app.reasons
        guard reasons.isEmpty else { return .failure("Nothing was changed. " + reasons.joined(separator: " ")) }
        // A proposal that draws what the saved layout draws is the saved layout, however it is spelled out: it leaves
        // no draft, and nothing a person could save that changes nothing. The same holds for a screen's style, and a
        // proposal is whole: a style it leaves out is the style the screen has saved. A call that gives the style alone
        // is no layout, so the layout stays as it is, saved or as a draft, the screen's own or the shared one; the
        // layout it would read as, the default, has nothing to check. A call that gives the app's settings alone is
        // neither a layout nor a style, so both stay as they are.
        let styleAlone = Self.givesStyleAlone(arguments), appAlone = Self.givesAppAlone(arguments)
        let asked = proposal.reduced(against: existing)
        switch scope {
        case .shared:
            if !styleAlone, !appAlone {
                layout.propose(asked == layout.current.reduced(against: existing) ? layout.current : asked)
            }
        case .screen(let key):
            if !styleAlone, !appAlone { layout.propose(choice(asked, for: key, against: existing), for: key) }
            if !appAlone { proposeStyle(style, for: key) }
        }
        proposeApp(app)
        // A layout edit holds its screen's panel open; the style alone holds none and closes the panel an edit of the
        // scope would hold, so the HUD it restyled shows. For the shared layout, which takes no style, it sets nothing.
        // The app's settings alone hold nothing and close nothing: nothing they change lies under the panel, the marks
        // a dock or a Simple panel keeps over it included, and the bar in a panel already open shows their draft.
        if styleAlone {
            if let key = heldScreen(for: scope, among: attached) { holds.pause(key) }
        } else if !appAlone {
            holdForEdit(scope, before: before, among: attached)
        }
        if appAlone {
            return .result(panel(scope, among: attached, note: appNote() + " A call with only app leaves every layout and "
                                 + "every screen's HUD style as it is.", proposed: true))
        }
        var note = proposalNote(scope, among: attached)
        if style.placedClear {
            note += " The dock it becomes starts at position 0, the top left, clear of the notch, as Settings places one; "
                + "another position can be proposed once the user has saved it."
        }
        if let glass = style.keptForGlass, let key = scope.screenKey {
            note += " The resting HUD of \(name(of: key, among: attached)) would be solid, so its glass style "
                + "\"\(glass.rawValue)\" is kept, as Settings keeps it, and takes effect once its material is "
                + "\"glass\"."
        }
        for path in projects.paths {
            note += " \"\(path.given)\" names the project \"\(path.id)\", which sessions.projects lists by that id."
        }
        if styleAlone {
            note += scope.screenKey.map { " The layout of the panel on \(name(of: $0, among: attached)) is unchanged" }
                ?? " The shared layout is unchanged"
            note += ": a call with only hud style fields leaves the layout as it is."
        }
        if app.given { note += " " + appNote() }
        return .result(panel(scope, among: attached, note: note, proposed: true))
    }

    /// Whether a call gives a screen's style alone: beside `screen`, `revision` and the app's settings (`app`) it names
    /// only a `hud`, every property of which is a style field, `reset` or a transparency the screen keeps included, and
    /// none `agents`. A key given as null is not given, so a hud that gives nothing, like a call without one, is the
    /// default layout.
    private static func givesStyleAlone(_ arguments: [String: JSONValue]) -> Bool {
        let taken: Set<String> = ["screen", "revision", "hud", "app"]
        let style = Set(HUDStyle.names + [HUDStyle.resetName] + HUDStyle.KeptTransparency.allCases.map(\.rawValue))
        let named = arguments.filter { $0.value != .null }.keys
        guard named.allSatisfy({ taken.contains($0) }), let hud = arguments["hud"]?.objectValue else { return false }
        let given = hud.filter { $0.value != .null }.keys
        return !given.isEmpty && given.allSatisfy { style.contains($0) }
    }

    /// What a screen is proposed for a layout sent for it: its saved own layout where the layout draws that; the shared
    /// layout where the screen draws the shared layout, saved or as its draft, and the layout draws what that draws, so
    /// the layout `get_hud` returned for such a screen, sent back, changes nothing; else the layout as its own.
    private func choice(_ asked: PanelLayout, for key: String, against existing: [PanelBlockInfo]) -> PanelScreenLayout {
        let own = layout.ownLayout(of: key)
        if let own, asked == own.reduced(against: existing) { return .own(own) }
        if own == nil || layout.usesShared(key), asked == layout.effective.reduced(against: existing) { return .shared }
        return .own(asked)
    }

    /// The layout a scope's panel draws now, a draft included.
    func effective(_ scope: PanelLayoutScope) -> PanelLayout {
        guard case .screen(let key) = scope else { return layout.effective }
        return layout.effective(for: key)
    }

    /// The layout a scope keeps saved: the shared one, or the screen's own, else the shared one it uses.
    private func savedLayout(_ scope: PanelLayoutScope) -> PanelLayout {
        guard case .screen(let key) = scope else { return layout.current }
        return layout.ownLayout(of: key) ?? layout.current
    }

    /// The agents a scope's layout can list now: an attached screen's own in `mode`, the mode it has now where none is
    /// named, else what any attached screen can show.
    func agents(for scope: PanelLayoutScope, among attached: [PanelScreen], in mode: HUDMode? = nil) -> [String] {
        if case .screen(let key) = scope, let screen = attached.first(where: { $0.key == key }) {
            return screen.agents(in: mode ?? screen.mode)
        }
        return VendorMark.merged(attached.flatMap(\.agents))
    }

    private func whose(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> String {
        if case .screen(let key) = scope, attached.contains(where: { $0.key == key }) {
            return "the resting HUD of \(name(of: key, among: attached))"
        }
        return "any attached screen's resting HUD"
    }

    private func proposalNote(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> String {
        switch scope {
        case .shared:
            guard layout.hasDraft else { return Self.savedNote }
            let own = attached.filter { !layout.usesShared($0.key) }.map(\.name)
            return own.isEmpty ? Self.draftNote
                : Self.draftNote + " Screens with a layout of their own do not show it: \(PanelLayout.quoted(own))."
        case .screen(let key):
            let name = name(of: key, among: attached)
            let draft = layout.draft(of: key)
            if layout.styleDraft(of: key) != nil { return styleNote(name, of: key) + unlistedNote(of: key) }
            guard let draft else {
                return "This is what \(name) already has saved, so there is no draft for it."
            }
            let what = draft == .shared ? "an unsaved draft that uses the shared layout" : "an unsaved draft of its own"
            guard attached.contains(where: { $0.key == key }) else {
                return "Kept for \(name) as \(what). It shows in that display's panel if it returns before Agent HUD "
                    + "restarts, which drops it; the user keeps it with Save in that panel."
            }
            return "Shown in the panel on \(name) as \(what). The user keeps it by hovering the Agent HUD on that display "
                + "and pressing Save in the panel; it is dropped when Agent HUD restarts."
        }
    }

    /// What a proposal that leaves a screen a style draft did: its resting HUD shows the style at once, beside the
    /// layout draft its panel draws, its own or the shared one's, if any. A person keeps both with one Save; moving that
    /// HUD or changing that display in Settings drops the style first.
    private func styleNote(_ name: String, of key: String) -> String {
        let dropped = "and the style also when the user moves that HUD or changes that display in Settings."
        let both = "The user keeps both by hovering the Agent HUD on that display and pressing Save in the panel; they are "
            + "dropped when Agent HUD restarts, " + dropped
        if let draft = layout.draft(of: key) {
            let what = draft == .shared ? "an unsaved draft that uses the shared layout" : "an unsaved draft of its own"
            return "Shown on \(name) as \(what), with its resting HUD's style as an unsaved draft too. " + both
        }
        guard layout.draftScope(on: key) == nil else {
            return "The resting HUD of \(name) shows its style as an unsaved draft, and its panel the shared layout's "
                + "unsaved draft. " + both
        }
        return "The resting HUD of \(name) shows its style as an unsaved draft; its panel's layout is the one it has "
            + "saved. The user keeps the style by hovering the Agent HUD on that display and pressing Save in the "
            + "panel; it is dropped when Agent HUD restarts, " + dropped
    }

    /// What a style draft that brings a version back draws beyond the fields its hud lists, as Save writes it: a sentence
    /// after the note, or nothing where it draws those fields alone.
    private func unlistedNote(of key: String) -> String {
        guard let draft = layout.styles[key] else { return "" }
        let settings = store.settings.settings
        guard draft.restoresUnlisted(over: settings.placement(on: key, hasNotch: draft.hasNotch), glow: settings.glow(on: key),
                                     defaultGlow: settings.glow) else { return "" }
        return " Beside the fields its hud lists, the draft brings back what no hud field sets, such as the transparency "
            + "kept for the material the HUD does not draw, and Save writes that too."
    }

    private func discardNote(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> String {
        guard case .screen(let key) = scope else {
            // The screens' own drafts are theirs: dropping the shared draft leaves them, so the note names them.
            let kept = layout.screenKeys.filter { layout.hasDraft(.screen($0)) }.map { key in
                attached.first { $0.key == key }?.name ?? key
            }
            guard !kept.isEmpty else { return "There is no draft now. The panel shows the saved layout." }
            return "The shared layout has no draft now. Screens with a draft of their own keep it: "
                + "\(PanelLayout.quoted(kept)); drop one with that screen's key."
        }
        let name = name(of: key, among: attached, starting: true)
        guard layout.usesShared(key) else { return "\(name) has no draft now. Its panel shows its own saved layout." }
        guard layout.hasDraft else { return "\(name) has no draft now. Its panel shows the saved shared layout." }
        return "\(name) has no draft of its own now. It uses the shared layout, whose unsaved draft it still shows; "
            + "drop that with screen \"all\"."
    }
}
