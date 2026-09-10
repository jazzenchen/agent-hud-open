import Foundation

/// Source of quota, session and history data: demo, Claude Code, Codex and DeepSeek Harness.
/// Never depends on AppKit.
public protocol UsageProvider: Sendable {
    /// - agents: the user's ordered agent list; the provider fills what it knows and skips the rest.
    /// - historyHours: how many hourly buckets to load, including the current partial hour.
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport
}

public struct UsageProviderError: Error, Hashable, Sendable, LocalizedError {
    public let message: String

    public init(_ message: String) {
        self.message = message
    }

    public var errorDescription: String? { message }
}
