import AgentHUDSupport
import Foundation

/// How the tools hold a screen's panel open while an agent edits it (`PanelHolds`): each edit of a layout, and each drop
/// of a draft, starts or renews the hold on the screen whose panel draws it, a call that changes a screen's resting HUD
/// alone closes that panel for now, `finish_hud_edit` lets go, and `get_hud` tells which screens are held and for
/// how long. A hold is held for an attached screen only, which has a panel to open.
extension HUDTools {
    var holds: PanelHolds { layout.holds }

    /// The attached screen whose panel an edit of `scope`'s layout holds open: the screen's own; for the shared layout,
    /// the one the person is at where it draws the shared layout, else the first attached screen that does. Nil where no
    /// such screen is attached.
    func heldScreen(for scope: PanelLayoutScope, among attached: [PanelScreen]) -> String? {
        switch scope {
        case .screen(let key):
            return attached.contains { $0.key == key } ? key : nil
        case .shared:
            let drawing = attached.filter { layout.usesShared($0.key) }
            return (drawing.first(where: \.hasPointer) ?? drawing.first)?.key
        }
    }

    /// Starts or renews the hold an edit of `scope`'s layout puts on its screen's panel, with the agent the edit touched
    /// first, whose own card a dock that separates its agents opens.
    /// - before: the layout the scope's panel drew before the edit.
    func holdForEdit(_ scope: PanelLayoutScope, before: PanelLayout, among attached: [PanelScreen]) {
        guard let key = heldScreen(for: scope, among: attached) else { return }
        holds.hold(key, agent: touchedAgent(from: before, to: effective(scope)))
    }

    /// The agent an edit from `before` to `after` touches first, by vendor: the first block, in the order the panel
    /// shows them after it, then the hidden ones, whose settings or place changed. A block's place is where it is shown,
    /// if at all, among the blocks shown both before and after, so a block that is hidden, or whose rows are all hidden,
    /// is the one touched, not the blocks after it that move up into its place. Nil where no block did, as for a change
    /// of the style, the sections, the session rows or the token chart alone.
    func touchedAgent(from before: PanelLayout, to after: PanelLayout) -> String? {
        let existing = store.view.panelBlocks
        let was = PanelLayoutPlan(layout: before, blocks: existing).blocks.map(\.key)
        let now = PanelLayoutPlan(layout: after, blocks: existing).blocks.map(\.key)
        let wasAmongBoth = was.filter(now.contains), nowAmongBoth = now.filter(was.contains)
        let keys = now + existing.map(\.key).filter { !now.contains($0) }
        let touched = keys.first { key in
            before.blocks[key] != after.blocks[key] || wasAmongBoth.firstIndex(of: key) != nowAmongBoth.firstIndex(of: key)
        }
        guard let touched else { return nil }
        // A quota block's key is its vendor; an API balance's is its provider's, whose accounts name their vendor.
        return store.enabledBilling.first { $0.panelBlockKey == touched }?.vendor ?? touched
    }

    /// What `get_hud` tells of the hold on a screen: whether one is on, and while it is, the whole seconds it has left
    /// and whether a call that restyled the resting HUD alone closed the panel for now.
    func holdListing(_ key: String) -> [String: JSONValue] {
        guard let hold = holds.hold(on: key), let left = holds.remaining(on: key) else { return ["held": .bool(false)] }
        return ["held": .bool(true), "holdSecondsLeft": .integer(Int64(left.rounded(.up))), "holdPaused": .bool(hold.paused)]
    }

    /// `finish_hud_edit`: ends the hold on a screen, or with "all" on every screen, and returns the panel with a note
    /// that says which panels it let go of and whether a draft still waits.
    func finish(_ scope: PanelLayoutScope, among attached: [PanelScreen]) -> [String: JSONValue] {
        var note: String
        switch scope {
        case .screen(let key):
            let name = name(of: key, among: attached)
            let held = holds.hold(on: key) != nil
            holds.end(key)
            note = held ? "The panel on \(name) is no longer held open: it closes unless the user's pointer is on it."
                : "The panel on \(name) was not held open, so nothing changed."
        case .shared:
            let held = holds.screens.map { key in attached.first { $0.key == key }?.name ?? key }
            holds.endAll()
            let names = PanelLayout.quoted(held)
            note = held.isEmpty ? "No panel was held open, so nothing changed."
                : held.count == 1 ? "No panel is held open now: the panel on \(names) closes unless the user's pointer is on it."
                : "No panel is held open now: the panels on \(names) close unless the user's pointer is on them."
        }
        // Letting go of every panel leaves every draft: the shared one, and any attached screen's own.
        if case .screen = scope, unsaved(scope) {
            note += " The draft waits for the user to press Save in the panel."
        } else if scope == .shared, layout.hasDraft || attached.contains(where: { layout.hasDraft(on: $0.key) }) {
            note += " Drafts wait for the user to press Save in the panel."
        }
        return panel(scope, among: attached, note: note)
    }
}
