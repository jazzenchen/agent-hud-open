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
