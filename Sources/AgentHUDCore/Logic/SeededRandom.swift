import Foundation

/// The design prototype's linear congruential generator, ported bit-for-bit so demo charts match the mock.
/// `s = (s * 9301 + 49297) % 233280; value = s / 233280`
public struct SeededRandom: Hashable, Sendable {
    private var state: Int

    public init(seed: Int) {
        state = seed
    }

    public mutating func next() -> Double {
        state = (state * 9301 + 49297) % 233280
        return Double(state) / 233280
    }
}
