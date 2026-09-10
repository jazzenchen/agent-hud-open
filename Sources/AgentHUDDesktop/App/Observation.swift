import Foundation
import Observation

/// Re-arming observation loop for AppKit code that is not a SwiftUI view.
/// `read` touches the observable properties you care about; `onChange` runs on the main actor after each change.
@MainActor
public func observeChanges(_ read: @escaping @MainActor () -> Void, onChange: @escaping @MainActor () -> Void) {
    withObservationTracking {
        read()
    } onChange: {
        Task { @MainActor in
            onChange()
            observeChanges(read, onChange: onChange)
        }
    }
}

/// Observe a value within a larger observable property without reacting to unrelated field changes.
@MainActor
public func observeChanges<Value: Equatable>(_ read: @escaping @MainActor () -> Value, onChange: @escaping @MainActor () -> Void) {
    var previous = read()
    observeChanges({ _ = read() }, onChange: {
        let next = read()
        guard next != previous else { return }
        previous = next
        onChange()
    })
}
