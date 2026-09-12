import Foundation

/// Local activity plus the latest available account readings.
/// Never depends on AppKit.
public protocol UsageProvider: Sendable {
    /// Refresh slow account APIs independently of local activity. Providers own their request cadence
    /// and expose account failures in the next report's notices.
    func refreshAccountUsage(historyHours: Int) async

    /// - agents: the user's ordered agent list; the provider fills what it knows and skips the rest.
    /// - historyHours: how many hourly buckets to load, including the current partial hour.
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport
}

extension UsageProvider {
    public func refreshAccountUsage(historyHours: Int) async {}
}

public struct UsageProviderError: Error, Hashable, Sendable, LocalizedError {
    public let message: String

    public init(_ message: String) {
        self.message = message
    }

    public var errorDescription: String? { message }
}
