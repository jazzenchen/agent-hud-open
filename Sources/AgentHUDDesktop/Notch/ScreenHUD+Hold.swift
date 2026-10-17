import AgentHUDCore
import Foundation

/// What one screen's panel follows of an agent's hold on it (`PanelHolds`).
struct PanelHoldState: Equatable {
    /// Whether the panel is open because the hold keeps it so.
    var open = false
    /// Whether the pointer came onto the HUD while the hold was on, or was on the open panel the hold took over: leaving
    /// it again is the person taking over. A panel that opens or grows under a pointer at rest is not the pointer coming
    /// onto it.
    var entered = false
}

/// A screen's panel under an agent's hold: open whether or not the pointer is over it, for as long as the hold is on.
///
/// The hold opens the panel on a path of its own (`open(for:)`, through `transition(_:reads:)` with `reads` false),
/// without the account read a hover's opening starts, so an agent's edit never wakes collection; the panel draws the
/// readings the store holds. It opens as `forceOpen()` does, so the island grows, a detached card's shadow waits for the
/// card and the glass joins as they do for a hover. A call that restyled the resting HUD alone pauses the hold and closes
/// the panel, so the HUD shows; the next layout edit opens it again. When the hold ends, by its time, by the agent's
/// `finish_hud_edit`, or by a person taking over, the panel closes as a hovered one does once the pointer has left it,
/// unless the pointer is on it, and from then on it is a hovered panel.
///
/// A person takes over, which ends the hold at once, by pressing Save or Discard in the panel or any of Settings' layout
/// buttons (`PanelLayoutStore`), by changing that display's placement or glow in Settings
/// (`SettingsStore.updateScreen(_:_:)`), by bringing the pointer onto the HUD and taking it away again, by clicking
/// anywhere but on the open panel (`personClicked()`), by pressing Escape while Agent HUD has the keyboard
/// (`IslandController`), by a click on the panel that leads elsewhere or closes it (Statistics, Settings, a session, an
/// event's choices: `forceCollapse()`), by opening another card, another agent's on a dock that separates its agents
/// (`pinAgent()`, `followHoveredMark(_:)`) or another request or reply (`selectRequest(_:)`, `openAlert()`), and by
/// dragging the HUD (`beginMoving(at:)`). An event's card on show keeps the hold from opening the panel until the card
/// goes, and a panel the hold keeps open stays open when an event's card in it goes.
extension ScreenHUD {
    /// Follows this screen's hold from now on: as it is, and at every change of it.
    func followHolds() {
        trackChanges({ [weak self] () -> PanelHolds.Hold? in
            guard let self else { return nil }
            return self.panelHolds.hold(on: self.key)
        }, onChange: { [weak self] in self?.followHold() })
        followHold()
    }

    /// Opens the panel for a hold, closes it while the hold is paused, and lets go of it when the hold has ended.
    func followHold() {
        guard !isClosed else { return }
        guard let hold = panelHolds.hold(on: key) else {
            let wasOpen = panelHold.open
            panelHold = PanelHoldState()
            if wasOpen {
                // The panel may have changed under a pointer that has not moved since it was last looked at.
                samplePointer()
                letGo()
            }
            return
        }
        guard hold.paused else { return open(for: hold) }
        guard panelHold.open else { return }
        panelHold.open = false
        // The panel covers the HUD an agent restyled. A pointer on it keeps it, as it keeps a hovered panel.
        samplePointer()
        letGo()
    }

    /// Opens the panel for `hold`, without reading the accounts: the hold's own way in, beside a hover's. A panel the
    /// hover is closing, the pointer having left it, is kept open, and one the pointer is on is the person's already, so
    /// the pointer leaving it takes over. On a dock that separates its agents it opens as the whole card of the agent
    /// the edits touched first, else of the first agent the dock shows, unless the pointer is on a mark the person chose.
    private func open(for hold: PanelHolds.Hold) {
        guard previewPlacement == nil, activeAlert == nil else { return }
        if !panelHold.open { panelHold = PanelHoldState(open: true, entered: pointerInside && machine.isOpen) }
        let marks = agentMarks
        var changed = false
        if let card = marks.first(where: { $0.key == hold.agent.map(AgentArtwork.markKey) }) ?? marks.first {
            let chosen = pointerInside && marks.contains { $0.key == hoveredMark }
            if !chosen, hoveredMark != card.key {
                hoveredMark = card.key
                changed = true
            }
            if !pinned {
                pinned = true
                changed = true
            }
        }
        let wasOpen = machine.isOpen
        // Opening re-applies the HUD itself; a panel already open is applied again only for another card.
        if machine.state != .open { transition(machine.reduce(.forceOpen, config: config), reads: false) }
        if wasOpen, changed { apply(animated: true) }
    }

    /// The last event's card has gone: a hold that is on opens the panel it kept closed meanwhile. It is looked at once
    /// the click that took the card away has done the rest of what it does, which may be taking over.
    func followHoldAfterAlert() {
        guard panelHolds.hold(on: key) != nil else { return }
        Task { [weak self] in self?.followHold() }
    }

    /// The panel stops being held: one the pointer is on stays open as a hovered one, and any other closes as a hovered
    /// panel does once the pointer has left it, after the collapse delay. A panel the hover is already closing is left
    /// to it.
    private func letGo() {
        guard !pointerInside, machine.state == .open else { return }
        transition(machine.reduce(.pointerExited(at: Date()), config: config))
    }

    /// The pointer's part in a hold: coming onto the HUD and leaving it again is the person taking over, which ends the
    /// hold; the hover, which saw the pointer come and go, then closes the panel as it closes a hovered one. `came` is
    /// whether the pointer itself moved onto the HUD (`samplePointer()`): a held panel that opened or grew under a
    /// pointer at rest, which a person typing to the agent may have left anywhere, is not the pointer coming onto it, so
    /// the pointer moving off it again leaves the hold on.
    func followHoldPointer(_ inside: Bool, came: Bool) {
        guard isHeld else { return }
        if inside {
            if came { panelHold.entered = true }
        } else if panelHold.entered {
            endHold()
        }
    }

    /// A click anywhere, which the host hears of: one off this HUD's open panel is the person taking over, which ends
    /// the hold. A click on the panel is the panel's own: a button that leads elsewhere ends it there, and the pointer
    /// that made it ends it when it leaves.
    func personClicked() {
        guard isHeld, !Self.containsPointer(mouseLocation(), in: hoverRegion) else { return }
        personTookOver()
    }

    /// A person acting on this HUD ends its hold at once; the panel then behaves as a hovered panel does: one the hold
    /// kept open closes unless the pointer is on it.
    func personTookOver() {
        let wasOpen = panelHold.open
        endHold()
        if wasOpen { letGo() }
    }

    /// Ends this screen's hold and leaves the panel as it is, for a person whose pointer is on the HUD or who closes or
    /// moves it at once; `followHold()` then sees nothing left to let go of.
    func endHold() {
        panelHold = PanelHoldState()
        panelHolds.end(key)
    }

    /// Whether a hold is on this screen's panel, or keeps it open.
    var isHeld: Bool { panelHold.open || panelHolds.hold(on: key) != nil }

    /// Whether a hold keeps this screen's panel open now.
    var isHeldOpen: Bool { panelHold.open && machine.isOpen }
}
