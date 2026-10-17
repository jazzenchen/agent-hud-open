import AgentHUDSupport
import Foundation
import Observation

/// Which layout document an action is for: the shared one, or one screen's own, by its screen key.
public enum PanelLayoutScope: Hashable, Sendable {
    case shared
    case screen(String)

    /// The screen's key; nil for the shared layout.
    public var screenKey: String? {
        if case .screen(let key) = self { return key }
        return nil
    }
}

/// The hover panel's layouts on this Mac: a shared layout, and for any screen an own layout it uses whole instead. Each
/// is a document in three states: the saved layout (`current`), the one saved before it (`previous`), and a change an
/// agent proposed that no person has saved yet (`draft`). A screen's draft can also be to use the shared layout again.
/// A screen can also have a style draft (`HUDStyle`): an agent's change to its resting HUD, drawn over that screen's
/// settings, which stay the saved truth. The app has one draft of its own beside them (`AppDraft`): an agent's change to
/// the app's own settings, drawn over them for every screen.
///
/// A screen's panel draws `effective(for:)`, so a draft shows at once. Only a person keeps one: `save(_:)` makes the
/// draft the saved layout and the saved one the previous, and writes a screen's style draft into the settings. A
/// screen's versions cover its whole look: the previous version keeps the screen's own placement and glow where they
/// differ from the saved version's, and going back to it restores those too, as an agent's proposal of it does as a
/// draft of the layout and the style. Going to the default is two things, the layout's (`reset(_:)`) and the look's
/// (`resetLook(_:)`), each a version of its own, so going back after either undoes it. The app's draft has no versions:
/// the bar's Save writes it into the settings and its Discard drops it, with the drafts that panel shows. Drafts live in
/// memory, so a relaunch drops them. The saved layouts are one value in the defaults the store was given (`PanelLayoutStorage`),
/// written when any of them changes, and a screen that is not attached keeps its own layout for when it returns.
/// Nothing here reads a provider or touches the agent list, and only a person's action writes the settings. A person's
/// choice ends the agents' holds on the panels it reaches (`holds`): Save, Revert to Previous and both resets here, and
/// the bar's Save and Discard (`savePanel(on:)`, `discardPanel(on:)`); `discard(_:)` alone is an agent's too, and leaves
/// them. The methods without a scope act on the shared layout.
@MainActor
@Observable
public final class PanelLayoutStore {
    /// The shared saved layout, normalized.
    public private(set) var current: PanelLayout
    /// The shared layout that was saved before `current`, normalized; nil while there has been none.
    private(set) var previous: PanelLayout?
    /// An agent's change to the shared layout waiting for a person to save or discard it, normalized and never equal to
    /// `current`.
    private(set) var draft: PanelLayout?
    /// The screens' own documents, by screen key, each normalized and never empty.
    private(set) var screens: [String: PanelScreenDocument]
    /// The screens' style drafts, by screen key, each changing something.
    private(set) var styles: [String: PanelStyleDraft] = [:]
    /// The app's draft: an agent's change to the app's own settings, drawn over them for every screen
    /// (`SettingsStore.effectiveSettings`), changing something; nil without one.
    public private(set) var appDraft: AppDraft?
    /// Counts the changes since launch, from 0, across every document: one more each time a saved layout, a previous
    /// one, a draft, a style draft or the app's draft changes, so a caller can tell that something changed since it last
    /// read.
    private(set) var revision = 0
    /// The holds agents' edits put on the screens' panels, which a person's choice about a layout ends (`PanelHolds`).
    public let holds = PanelHolds()

    private let defaults: UserDefaults
    /// The settings a style draft is drawn over and saved into, and a version's style is read from and restored to; set
    /// by the `SettingsStore` that holds the store. Without them a style draft is held but a version keeps no style.
    @ObservationIgnored weak var owner: SettingsStore?

    /// The shared layout as the panels that use it draw it: its draft while there is one, else the saved layout.
    public var effective: PanelLayout { draft ?? current }
    /// Whether the shared layout has a draft.
    public var hasDraft: Bool { draft != nil }
    /// Whether the shared layout has a previous one to go back to.
    public var canRevert: Bool { previous != nil }

    /// Reads the saved layouts from `defaults` (`SettingsStore.Keys.panelLayout`), each side and each screen on its
    /// own, so one that cannot be read does not lose the rest.
    public init(defaults: UserDefaults) {
        self.defaults = defaults
        let saved = PanelLayoutStorage.read(defaults.data(forKey: SettingsStore.Keys.panelLayout))
        current = saved.current
        previous = saved.previous
        screens = saved.screens
    }

    // MARK: Reading a screen

    /// The layout the panel on `screen` draws: the screen's draft or its saved own layout where it has one, else the
    /// shared layout's draft or saved layout. Nil, or a screen without a document, is the shared layout.
    public func effective(for screen: String?) -> PanelLayout {
        guard let screen, let document = screens[screen] else { return effective }
        return (document.draft ?? document.current).layout ?? effective
    }

    /// Whether the panel on `screen` draws the shared layout now, a draft included.
    public func usesShared(_ screen: String?) -> Bool {
        guard let screen, let document = screens[screen] else { return true }
        return (document.draft ?? document.current) == .shared
    }

    /// The saved own layout of `screen`; nil while it uses the shared layout.
    public func ownLayout(of screen: String) -> PanelLayout? { screens[screen]?.current.layout }

    /// The draft of `screen`'s own document: a layout, or `.shared` to use the shared layout; nil without one.
    public func draft(of screen: String) -> PanelScreenLayout? { screens[screen]?.draft }

    /// The choice saved before `screen`'s saved one; nil while there has been none.
    func previous(of screen: String) -> PanelScreenLayout? { screens[screen]?.previous }

    /// The style draft of `screen`: an agent's change to its resting HUD, drawn over its settings; nil without one.
    public func styleDraft(of screen: String) -> HUDStyle? { styles[screen]?.style }

    /// Whether `screen` has a style draft.
    public func hasStyleDraft(_ screen: String?) -> Bool { screen.map { styles[$0] != nil } ?? false }

    /// The layout document whose draft the panel on `screen` shows, which the Save and Discard in its bar act on with the
    /// screen's style draft: the screen's own draft, else the shared draft while the screen draws the shared layout; nil
    /// when it shows none.
    public func draftScope(on screen: String?) -> PanelLayoutScope? {
        if let screen, screens[screen]?.draft != nil { return .screen(screen) }
        return usesShared(screen) && draft != nil ? .shared : nil
    }

    /// Whether the panel on `screen` shows a draft and the bar that says so: a layout draft, the screen's style draft, or
    /// the app's draft, which every screen's panel shows, so that its Save and Discard stay on the HUD whatever the draft
    /// hides, the menu-bar icon included.
    public func hasDraft(on screen: String?) -> Bool {
        draftScope(on: screen) != nil || hasStyleDraft(screen) || appDraft != nil
    }

    /// Whether a scope has a draft of its own: for a screen, a layout draft or a style draft.
    func hasDraft(_ scope: PanelLayoutScope) -> Bool {
        switch scope {
        case .shared: draft != nil
        case .screen(let key): screens[key]?.draft != nil || styles[key] != nil
        }
    }

    /// Whether a scope has a previous layout to go back to.
    public func canRevert(_ scope: PanelLayoutScope) -> Bool {
        switch scope {
        case .shared: previous != nil
        case .screen(let key): screens[key]?.previous != nil
        }
    }

    /// The screens with a document of their own, saved or a draft, or a style draft, in key order.
    var screenKeys: [String] { Set(screens.keys).union(styles.keys).sorted() }

    // MARK: An agent's change

    /// Shows a layout as the shared draft, without saving it; a layout that is the saved one clears the draft instead.
    /// Returns whether a draft now waits.
    @discardableResult
    public func propose(_ layout: PanelLayout) -> Bool {
        let proposed = layout.normalized()
        set(current: current, previous: previous, draft: proposed == current ? nil : proposed)
        return hasDraft
    }

    /// Shows a choice as `screen`'s draft, without saving it: a layout of its own, or the shared layout. A choice that
    /// is the saved one clears the draft instead. Returns whether a draft now waits for the screen.
    @discardableResult
    public func propose(_ choice: PanelScreenLayout, for screen: String) -> Bool {
        var document = screens[screen] ?? PanelScreenDocument()
        let proposed = choice.normalized()
        document.draft = proposed == document.current ? nil : proposed
        set(screen: screen, document)
        return document.draft != nil
    }

    /// Shows a style as `screen`'s style draft, without saving it, drawn over that screen's settings at once; an empty
    /// style drops the style draft instead, and the style a waiting draft already lists keeps that draft as it is, so
    /// what an agent read, sent back, changes nothing. The style is taken as given: an agent's proposal is reduced to
    /// what it changes first (`HUDTools`). Returns whether a style draft now waits for the screen.
    /// - hasNotch: whether the screen has a notch, which decides the placement a screen without one of its own saved
    ///   draws, and so the one a Save writes the style over.
    @discardableResult
    public func proposeStyle(_ style: HUDStyle, for screen: String, hasNotch: Bool) -> Bool {
        if let waiting = styles[screen], waiting.style == style, waiting.hasNotch == hasNotch { return true }
        set(screen: screen, screens[screen] ?? PanelScreenDocument(),
            style: style.isEmpty ? nil : PanelStyleDraft(style: style, hasNotch: hasNotch))
        return styles[screen] != nil
    }

    /// Drops `screen`'s style draft and leaves its layout draft: what a person's own edit of that screen, in Settings or
    /// by dragging its HUD, does first, so a person always wins.
    public func discardStyle(_ screen: String) {
        guard styles[screen] != nil else { return }
        set(screen: screen, screens[screen] ?? PanelScreenDocument(), style: nil)
    }

    /// Shows a change as the app's draft, without saving it, drawn over the app's settings at once for every screen; an
    /// empty change drops the app's draft instead. The change is taken as given: an agent's proposal is reduced to what it
    /// changes first (`HUDTools`). Returns whether an app draft now waits.
    @discardableResult
    public func proposeApp(_ draft: AppDraft) -> Bool {
        set(app: draft.isEmpty ? nil : draft)
        return appDraft != nil
    }

    /// Drops `field` from the app's draft, and the draft with it where that was all it set: what a person's own change of
    /// that setting in Settings does first, so a person always wins (`SettingsStore.updateSetting(_:_:)`).
    public func discardApp(_ field: AppDraft.Field) {
        guard let appDraft else { return }
        let left = appDraft.leaving(field)
        set(app: left.isEmpty ? nil : left)
    }

    /// Proposes the default view as a scope's layout: the shared layout's, or a screen's own, as the layout's Reset to
    /// Default gives it; a screen's style draft is left as it is. Returns whether a layout draft now waits for the scope.
    @discardableResult
    func proposeDefault(_ scope: PanelLayoutScope = .shared) -> Bool {
        switch scope {
        case .shared: propose(PanelLayout())
        case .screen(let key): propose(.own(PanelLayout()), for: key)
        }
    }

    /// Proposes a screen's default look as its style draft: the placement its hardware is given and the default glow,
    /// as the HUD style's Reset to Default gives them, drawn whole and written back by Save as that reset writes them.
    /// Its layout draft is left as it is. Returns whether a style draft now waits, which it does not where the screen
    /// already draws the defaults whole, what no style field names included.
    @discardableResult
    func proposeDefaultLook(for screen: String, hasNotch: Bool) -> Bool {
        proposeLook(ScreenLook(), for: screen, hasNotch: hasNotch)
        return styles[screen] != nil
    }

    /// Proposes a scope's previous version: for a screen, its layout and the style kept with it, as Revert to Previous
    /// brings them back. False, with nothing changed, when there is none; else whether a draft now waits for the scope.
    /// - hasNotch: for a screen, whether it has a notch; nil for one that is not attached, which has no resting HUD to
    ///   draw a style on, so its style is left as saved.
    @discardableResult
    func proposePrevious(_ scope: PanelLayoutScope = .shared, hasNotch: Bool? = nil) -> Bool {
        switch scope {
        case .shared:
            guard let previous else { return false }
            return propose(previous)
        case .screen(let key):
            guard let document = screens[key], let previous = document.previous else { return false }
            propose(previous, for: key)
            proposeLook(document.previousLook, for: key, hasNotch: hasNotch)
            return hasDraft(scope)
        }
    }

    /// Whether going back to a screen's previous version would change the placement or the glow it has saved: what a
    /// proposal of that version leaves as saved while the screen is not attached.
    func previousChangesLook(of screen: String) -> Bool {
        guard let now = look(of: screen), let version = screens[screen]?.previousLook else { return false }
        return version != now
    }

    /// Shows a version's look as `screen`'s style draft: drawn as that look, listed as the fields an agent reads that
    /// differ from what is saved, and written back by Save exactly as it was kept. Nil, a look the screen already draws,
    /// or a screen that is not attached (`hasNotch` nil) drops the style draft instead, so the proposal is whole. The
    /// whole placement and glow are weighed, as Revert to Previous and the HUD style's Reset to Default restore them, so
    /// a look that differs only in what no style field names, the transparency kept for the material not drawn, is a
    /// draft that lists no field.
    private func proposeLook(_ look: ScreenLook?, for screen: String, hasNotch: Bool?) {
        guard let look, let hasNotch, let settings = owner?.settings else { return discardStyle(screen) }
        let placement = look.placement ?? .default(hasNotch: hasNotch), glow = look.glow ?? settings.glow
        let saved = settings.placement(on: screen, hasNotch: hasNotch), savedGlow = settings.glow(on: screen)
        let draws = placement == saved && glow == savedGlow
        let style = HUDStyle(every: placement, glow: glow).reduced(placement: saved, glow: savedGlow)
        set(screen: screen, screens[screen] ?? PanelScreenDocument(),
            style: draws ? nil : PanelStyleDraft(style: style, hasNotch: hasNotch, restoring: look))
    }

    // MARK: A person's choice

    /// Keeps a scope's draft: it becomes the saved layout and the saved one the previous. A screen's draft to use the
    /// shared layout removes the screen's own layout, which becomes its previous. A screen's style draft is written into
    /// the settings in one change, a version it restores as that version was kept, with its layout draft, if any, as one
    /// new version, and the screen's own placement and glow before it are kept with the previous version where they
    /// differ from what is written. Nothing without a draft.
    public func save(_ scope: PanelLayoutScope = .shared) {
        holds.end(scope, in: self)
        switch scope {
        case .shared:
            guard let draft else { return }
            set(current: draft, previous: current, draft: nil)
        case .screen(let key):
            let document = screens[key] ?? PanelScreenDocument()
            let style = styles[key]
            guard document.draft != nil || style != nil else { return }
            let before = look(of: key)
            var after: ScreenLook?
            if let style, let before, let settings = owner?.settings { after = style.applied(to: before, defaultGlow: settings.glow) }
            let saved = PanelScreenDocument(current: document.draft ?? document.current, previous: document.current,
                                            previousLook: after == nil || after == before ? nil : before)
            set(screen: key, saved, style: nil)
            if let after { restore(after, for: key) }
        }
    }

    /// Drops a scope's draft; its panels show the saved layout again. A screen's style draft goes with it.
    public func discard(_ scope: PanelLayoutScope = .shared) {
        switch scope {
        case .shared:
            set(current: current, previous: previous, draft: nil)
        case .screen(let key):
            var document = screens[key] ?? PanelScreenDocument()
            document.draft = nil
            set(screen: key, document, style: nil)
        }
    }

    /// Goes back to a scope's previous version: the saved and the previous one change places, so reverting again undoes
    /// it. For a screen that takes in its style: a previous version that kept the screen's own placement and glow has
    /// them restored, and the ones they replace are kept with the version reverted from. Without a previous layout the
    /// saved one stays. Any draft of the scope, a style draft included, is dropped.
    public func revert(_ scope: PanelLayoutScope = .shared) {
        holds.end(scope, in: self)
        switch scope {
        case .shared:
            set(current: previous ?? current, previous: previous == nil ? nil : current, draft: nil)
        case .screen(let key):
            guard let document = screens[key] else { return discardStyle(key) }
            let before = look(of: key)
            var reverted = PanelScreenDocument(current: document.previous ?? document.current,
                                               previous: document.previous == nil ? nil : document.current)
            let restored = document.previous == nil ? nil : document.previousLook
            if let restored, restored != before { reverted.previousLook = before }
            set(screen: key, reverted, style: nil)
            if let restored { restore(restored, for: key) }
        }
    }

    /// Goes back to the default view: the saved layout becomes the previous one, unless it already is the default. For
    /// a screen, the default view becomes its own layout and its style stays as saved, so the previous version is the
    /// layout it replaces with the same style. Any draft of the scope, a style draft included, is dropped.
    public func reset(_ scope: PanelLayoutScope = .shared) {
        holds.end(scope, in: self)
        switch scope {
        case .shared:
            set(current: PanelLayout(), previous: current.isDefault ? previous : current, draft: nil)
        case .screen(let key):
            let document = screens[key] ?? PanelScreenDocument()
            let layout = PanelScreenLayout.own(PanelLayout())
            let unchanged = document.current == layout
            set(screen: key, PanelScreenDocument(current: layout, previous: unchanged ? document.previous : document.current,
                                                 previousLook: unchanged ? document.previousLook : nil), style: nil)
        }
    }

    /// Gives a screen the default look at once, the placement its hardware is given and the default glow, as the HUD
    /// style's Reset to Default does, in one change of the settings. Its style draft is dropped first; its layout, saved
    /// or as a draft, stays. The look it replaces is kept with the previous version, beside the same layout, so Revert
    /// to Previous brings it back. A screen that already draws the defaults, or a store without settings, keeps its
    /// versions as they are.
    public func resetLook(_ screen: String) {
        holds.end(screen)
        guard let before = look(of: screen), before != ScreenLook() else { return discardStyle(screen) }
        let document = screens[screen] ?? PanelScreenDocument()
        set(screen: screen, PanelScreenDocument(current: document.current, previous: document.current, draft: document.draft,
                                                previousLook: before), style: nil)
        restore(ScreenLook(), for: screen)
    }

    /// Lets `screen` use the shared layout again at once: its own layout becomes its previous one. Any draft of the
    /// screen, a style draft included, is dropped; its style stays as saved.
    public func useShared(_ screen: String) {
        holds.end(screen)
        guard let document = screens[screen] else { return discardStyle(screen) }
        let unchanged = document.current == .shared
        set(screen: screen, PanelScreenDocument(current: .shared, previous: unchanged ? document.previous : document.current,
                                                previousLook: unchanged ? document.previousLook : nil), style: nil)
    }

    // MARK: The Expanded panel's switches

    /// The four switches of Settings → Display → Expanded panel as the saved shared layout sets them.
    public var switches: PanelSwitches { current.switches }

    /// The switches as the layout `screen` keeps saved sets them, which Settings shows for the display chosen in
    /// Settings → Display: its own layout where it keeps one, else the shared one. Nil is the shared layout.
    public func switches(on screen: String?) -> PanelSwitches {
        (screen.flatMap(ownLayout(of:)) ?? current).switches
    }

    /// A person's switch in Settings → Display → Expanded panel, for the display chosen there: sets the layout that
    /// display keeps saved at once, its own where it keeps one, else the shared one, and a waiting draft of that layout
    /// with it, so that the panel shows the change either way, the switch that changed and nothing else. Like a person's
    /// own edit of a display, it is no version: the previous layout stays as it is, and Revert to Previous still goes back
    /// to it. Every other layout keeps its own switches; nil, or a display that uses the shared layout, sets the shared
    /// one, which every display without a layout of its own draws.
    public func setSwitches(on screen: String? = nil, _ change: (inout PanelSwitches) -> Void) {
        let was = switches(on: screen)
        var now = was
        change(&now)
        guard now != was else { return }
        guard let screen, let own = ownLayout(of: screen), var document = screens[screen] else {
            let saved = current.switching(from: was, to: now).normalized()
            let drafted = draft.map { $0.switching(from: was, to: now).normalized() }
            return set(current: saved, previous: previous, draft: drafted == saved ? nil : drafted)
        }
        let saved = own.switching(from: was, to: now).normalized()
        document.current = .own(saved)
        if let drafted = document.draft?.layout.map({ $0.switching(from: was, to: now).normalized() }) {
            document.draft = drafted == saved ? nil : .own(drafted)
        }
        set(screen: screen, document)
    }

    /// Carries the switches as settings an earlier version wrote kept them into every saved layout, the shared one and
    /// each screen's own, and the ones saved before them, so that each draws the panel it drew under those settings: a
    /// switch that was off hides its section, or the countdown, in each, which Settings turns on again for the display
    /// chosen there (`setSwitches(on:_:)`). Written once, at launch, before any panel reads a layout; the settings are
    /// then written without the switches, those it cannot read whole too, so it is not carried again.
    func carry(_ stored: PanelSwitches) {
        func carried(_ layout: PanelLayout) -> PanelLayout { layout.carrying(stored).normalized() }
        func carried(_ choice: PanelScreenLayout) -> PanelScreenLayout { choice.layout.map { .own(carried($0)) } ?? .shared }
        let shared = (carried(current), previous.map(carried))
        let screens = self.screens.mapValues { document in
            var document = document
            document.current = carried(document.current)
            document.previous = document.previous.map(carried)
            return document
        }
        guard shared.0 != current || shared.1 != previous || screens != self.screens else { return }
        current = shared.0
        previous = shared.1
        self.screens = screens
        persist()
    }

    // MARK: The settings

    /// The screen's own placement and glow as the settings keep them; nil without settings to read.
    private func look(of screen: String) -> ScreenLook? {
        guard let settings = owner?.settings else { return nil }
        return ScreenLook(placement: settings.screens[screen], glow: settings.screenGlow[screen])
    }

    /// Writes a screen's own placement and glow into the settings in one change; nil removes one, so the screen draws
    /// the default.
    private func restore(_ look: ScreenLook, for screen: String) {
        guard let owner, self.look(of: screen) != look else { return }
        owner.update {
            $0.screens[screen] = look.placement
            $0.screenGlow[screen] = look.glow
        }
    }

    // MARK: State

    /// Takes the shared document's three states: one revision when any of them differs, and one write when a saved one
    /// does.
    private func set(current: PanelLayout, previous: PanelLayout?, draft: PanelLayout?) {
        let saves = current != self.current || previous != self.previous
        guard saves || draft != self.draft else { return }
        if current != self.current { self.current = current }
        if previous != self.previous { self.previous = previous }
        if draft != self.draft { self.draft = draft }
        revision += 1
        if saves { persist() }
    }

    /// Takes a screen's document, which an empty one removes, and its style draft: one revision when either differs,
    /// and one write when what the document saves does.
    private func set(screen key: String, _ document: PanelScreenDocument, style: PanelStyleDraft?) {
        let old = screens[key] ?? PanelScreenDocument()
        guard document != old || style != styles[key] else { return }
        if document != old { screens[key] = document.isEmpty ? nil : document }
        if style != styles[key] { styles[key] = style }
        revision += 1
        if document.current != old.current || document.previous != old.previous || document.previousLook != old.previousLook {
            persist()
        }
    }

    /// Takes the app's draft: one revision when it differs.
    private func set(app draft: AppDraft?) {
        guard draft != appDraft else { return }
        appDraft = draft
        revision += 1
    }

    /// Writes the app's draft into the settings in one change and drops it; without settings it is dropped alone.
    func saveApp() {
        guard let appDraft else { return }
        if let owner { owner.update { $0 = appDraft.applied(to: $0) } }
        set(app: nil)
    }

    /// Drops the app's draft; the app draws its saved settings again.
    func discardApp() { set(app: nil) }

    /// Takes a screen's document and keeps its style draft.
    private func set(screen key: String, _ document: PanelScreenDocument) {
        set(screen: key, document, style: styles[key])
    }

    /// Writes every saved layout at once, with sorted keys; nothing to keep is no stored value.
    private func persist() {
        let saved = PanelLayoutStorage.Saved(current: current, previous: previous, screens: screens)
        if let data = PanelLayoutStorage.write(saved) {
            defaults.set(data, forKey: SettingsStore.Keys.panelLayout)
        } else {
            defaults.removeObject(forKey: SettingsStore.Keys.panelLayout)
        }
    }
}
