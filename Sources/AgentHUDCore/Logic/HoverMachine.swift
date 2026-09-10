import Foundation

/// Pure state machine for the notch hover: open after `hoverDelay`, collapse after `collapseDelay`.
/// The caller schedules one timer for the returned `deadline` and feeds `.timerFired` back in.
public struct HoverMachine: Hashable, Sendable {
    public enum State: Hashable, Sendable {
        case collapsed
        case opening(deadline: Date)
        case open
        case closing(deadline: Date)
    }

    public enum Event: Hashable, Sendable {
        case pointerEntered(at: Date)
        case pointerExited(at: Date)
        case timerFired(at: Date)
        case forceOpen
        case forceCollapse
    }

    public struct Config: Hashable, Sendable {
        public let hoverDelay: TimeInterval
        public let collapseDelay: TimeInterval

        public init(hoverDelay: TimeInterval, collapseDelay: TimeInterval) {
            self.hoverDelay = hoverDelay
            self.collapseDelay = collapseDelay
        }
    }

    public struct Transition: Hashable, Sendable {
        public let machine: HoverMachine
        /// When non-nil the caller must fire `.timerFired` at this time (replacing any earlier timer).
        public let deadline: Date?
    }

    public let state: State

    public init(state: State = .collapsed) {
        self.state = state
    }

    /// True while the panel content should be visible (open, or about to close).
    public var isOpen: Bool {
        switch state {
        case .open, .closing: return true
        case .collapsed, .opening: return false
        }
    }

    public func reduce(_ event: Event, config: Config) -> Transition {
        switch (state, event) {
        case (.collapsed, .pointerEntered(let now)):
            let deadline = now.addingTimeInterval(config.hoverDelay)
            return Transition(machine: HoverMachine(state: .opening(deadline: deadline)), deadline: deadline)

        case (.opening, .pointerExited):
            return Transition(machine: HoverMachine(state: .collapsed), deadline: nil)

        case (.opening(let deadline), .timerFired(let now)) where now >= deadline:
            return Transition(machine: HoverMachine(state: .open), deadline: nil)

        case (.open, .pointerExited(let now)):
            let deadline = now.addingTimeInterval(config.collapseDelay)
            return Transition(machine: HoverMachine(state: .closing(deadline: deadline)), deadline: deadline)

        case (.closing, .pointerEntered):
            return Transition(machine: HoverMachine(state: .open), deadline: nil)

        case (.closing(let deadline), .timerFired(let now)) where now >= deadline:
            return Transition(machine: HoverMachine(state: .collapsed), deadline: nil)

        case (_, .forceOpen):
            return Transition(machine: HoverMachine(state: .open), deadline: nil)

        case (_, .forceCollapse):
            return Transition(machine: HoverMachine(state: .collapsed), deadline: nil)

        default:
            return Transition(machine: self, deadline: nil)
        }
    }
}
