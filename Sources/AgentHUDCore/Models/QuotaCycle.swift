import Foundation

/// One occurrence of a quota window, anchored to the service's next reset.
public struct QuotaCycle: Hashable, Sendable {
    public let resetAt: Date
    public let duration: TimeInterval

    public init?(resetAt: Date?, duration: TimeInterval?) {
        guard let resetAt, let duration, duration.isFinite, duration > 0 else { return nil }
        self.resetAt = resetAt
        self.duration = duration
    }

    public var start: Date { resetAt.addingTimeInterval(-duration) }

    /// 5h windows use 15-minute buckets; weekly windows use hourly buckets.
    public var sampleInterval: TimeInterval {
        duration >= 24 * 3600 ? 3600 : min(15 * 60, duration / 2)
    }
}
