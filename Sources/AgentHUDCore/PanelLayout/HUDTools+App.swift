import AgentHUDSupport
import Foundation

/// What the tools read and propose of the app's own settings (`AppDraft`): `get_hud` lists them under `app` as the app
/// draws them now, the app's draft included, with `appUnsaved` beside them, and `draft_hud` takes them under the
/// same `app`, whatever screen the call names, since they hold for every screen.
extension HUDTools {
    /// The app part of a proposal: the app's settings it names under `app`. A call that names no app leaves the app's
    /// draft as it is, since an agent changes these settings only when the person asks for one, never while it arranges
    /// the panel. One that names it is whole: a field it leaves out is what the app has saved, so an app that gives
    /// nothing drops the draft, and the app `get_hud` lists, a draft's fields included, sent back, keeps the draft as it
    /// is. A field set to what is saved changes nothing and is no issue, a delay saved outside Settings' range included.
    /// The language and the language the panel's names are written in, which `get_hud` lists beside the fields, set
    /// nothing: each is taken where it repeats what is listed, and refused otherwise.
    @MainActor
    struct AppProposal {
        /// Whether the call names an app, which then replaces the app's draft.
        let given: Bool
        /// What the proposal asks, reduced to what it changes about what the app has saved.
        let reduced: AppDraft
        /// Why the app cannot be drawn as asked, as sentences, what the reading would forgive included.
        let reasons: [String]

        init(app value: JSONValue?, tools: HUDTools) {
            guard let value, value != .null else {
                given = false
                reduced = AppDraft()
                reasons = []
                return
            }
            given = true
            let fields = value.objectValue ?? [:]
            let saved = tools.store.settings.settings
            reduced = AppDraft(json: fields).reduced(against: saved)
            var found = AppDraft.misreadings(of: value)
            found += reduced.issues(against: saved).map { "\($0.path): \($0.message)" }
            let listed = tools.appListing()
            for name in AppDraft.readOnly {
                guard let sent = fields[name], sent != .null, sent != listed[name] else { continue }
                let what = name == "language" ? "the Language chosen in Settings, which only Settings changes"
                    : "the language the panel's names are written in now"
                found.append("app.\(name) is read only: it is \(what), \(listed[name].map(Self.written) ?? "none") as "
                             + "get_hud lists it. Leave it out, or give it as listed.")
            }
            reasons = found
        }

        private static func written(_ value: JSONValue) -> String { value.stringValue.map { "\"\($0)\"" } ?? "none" }
    }

    /// What `get_hud` lists of the app under `app`: every field as the app draws it now, the app's draft included, in an
    /// agent's words (`AppDraft`), and beside them the language chosen and the one the panel's names are written in,
    /// which no proposal sets.
    func appListing() -> [String: JSONValue] {
        let settings = store.settings.effectiveSettings
        let written: AppLanguage = L10n.resolved == .zhHans ? .zhHans : .en
        return AppDraft(every: settings).json.merging([
            "language": .string(settings.language.rawValue),
            "writtenIn": .string(written.rawValue),
        ]) { $1 }
    }

    /// Shows a proposal's app as the app's draft: what it changes, so one that changes nothing drops the draft. A call
    /// that names no app leaves the draft as it is.
    func proposeApp(_ app: AppProposal) {
        guard app.given else { return }
        layout.proposeApp(app.reduced)
    }

    /// Whether a call gives the app's settings alone: beside `screen` and `revision` it names only an `app`. A key given as
    /// null is not given, so an app given as null, like a call without one, is the default layout.
    static func givesAppAlone(_ arguments: [String: JSONValue]) -> Bool {
        let taken: Set<String> = ["screen", "revision", "app"]
        let named = arguments.filter { $0.value != .null }.keys
        return named.contains("app") && named.allSatisfy { taken.contains($0) }
    }

    /// What a proposal that names an app did with the app's settings, as a sentence after the note: they show as an
    /// unsaved draft on every screen, which the bar in any panel keeps or drops with the rest, or they are as saved. A
    /// draft that hides the menu-bar icon, or that asks for Option to open the panel, says how the bar is reached.
    func appNote() -> String {
        guard let draft = layout.appDraft else {
            return "The app's own settings are as they are saved, so there is no draft of them."
        }
        var note = "The app's own settings show as an unsaved draft on every display: the user keeps it by hovering the "
            + "Agent HUD on any of them and pressing Save in the panel, which keeps whatever else that panel shows unsaved "
            + "with it, or drops it with Discard; it is dropped when Agent HUD restarts, and a setting of it also when the "
            + "user changes that setting in Settings."
        if draft.showMenuBarIcon == false {
            note += " With the menu-bar icon hidden, Save, Discard and the Settings button stay in the HUD's panel."
        }
        if draft.pressOptionToOpen == true {
            note += " With Press Option to open on, the user holds Option while hovering the HUD to open that panel."
        }
        return note
    }
}
