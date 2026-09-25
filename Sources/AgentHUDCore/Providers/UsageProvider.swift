import Foundation

/// Local activity plus the latest available account readings.
/// Never depends on AppKit.
///
/// A session is running while its newest turn is in flight, however quiet its log goes; only an explicit end, an
/// interruption, or evidence that the client is gone ends it. Today only the DeepSeek provider has that evidence,
/// from the process table.
/// TODO: give every provider the same signal — a client heartbeat — so a killed agent stops reporting a running turn
/// instead of leaving one standing until its log is read again.
public protocol UsageProvider: Sendable {
    /// Refresh slow account APIs independently of local activity. Providers own their request cadence
    /// and expose account failures in the next report's notices.
    func refreshAccountUsage(historyHours: Int) async

    /// The account refresh as independent steps, such as one per vendor. The store runs them one at a time
    /// between local polls, so a slow account request never overlaps another read.
    var accountRefreshSteps: [AccountRefreshStep] { get }

    /// - agents: the user's ordered agent list; the provider fills what it knows and skips the rest.
    /// - historyHours: how many hourly buckets to load, including the current partial hour.
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int) async throws -> UsageReport

    /// Directories whose changes can alter the next local report. Nil when the provider cannot tell,
    /// so every poll reads its local data.
    var watchedDirectories: [URL]? { get }

    /// The parts of this provider that can be read on their own, each with what signals that it has new data.
    /// The collector reads a source only when one of its signals fires.
    var sources: [UsageSource] { get }

    /// Reads the named sources again and keeps every other source's last result; nil reads every source.
    func fetchUsage(agents: [AgentDescriptor], historyHours: Int, sources: Set<String>?) async throws -> UsageReport

    /// File paths observed by the collector before a read. Nil means its watch lost events, so a file-backed source
    /// must enumerate again. Providers without an incremental file index ignore this.
    func fileChanges(_ paths: Set<String>?) async

    /// When each source's last result changes with time alone, such as a live session that stops being live after a quiet
    /// interval. The collector reads a source again when one of its times passes; a source without times is left alone.
    func sourceChecks() async -> [String: [Date]]

    /// When each source's account steps are next worth running, given when each last ran. A quota window moves only
    /// while work runs, so a source is read while its work runs and left alone while it is quiet. A source this
    /// provider does not name runs every account interval, and `Date.distantFuture` asks for no reading at all.
    func accountChecks(since: [String: Date], now: Date) async -> [String: Date]

    /// Whether this provider can tell work from quiet on this Mac. A provider whose usage appears only in the account
    /// answer cannot, so its readings keep the account interval however quiet it looks.
    var seesLocalWork: Bool { get }
}

public typealias AccountRefreshStep = @Sendable (_ historyHours: Int) async -> Void

/// A part of a provider that is read on its own, and the signals that it has new data: changes under its directories,
/// its account steps, and the checks it asks for. Whatever a source does inside, the collector sees only these signals.
public struct UsageSource: Sendable {
    public let name: String
    /// Directories whose file changes need a read of this source. Nil when the source cannot tell, so it is read every
    /// poll interval instead.
    public let directories: [URL]?
    /// This source's account refresh, run by the account sweep. A finished step signals a read of this source.
    public let accountSteps: [AccountRefreshStep]

    public init(name: String, directories: [URL]?, accountSteps: [AccountRefreshStep]) {
        self.name = name
        self.directories = directories
        self.accountSteps = accountSteps
    }
}

extension UsageProvider {
    public func refreshAccountUsage(historyHours: Int) async {}
    public var accountRefreshSteps: [AccountRefreshStep] { [{ await self.refreshAccountUsage(historyHours: $0) }] }
    public var watchedDirectories: [URL]? { nil }
    /// A provider that does not split itself is one source; the collector derives its checks from the report it returned.
    public var sources: [UsageSource] { [UsageSource(name: "", directories: watchedDirectories, accountSteps: accountRefreshSteps)] }
    public func fetchUsage(agents: [AgentDescriptor], historyHours: Int, sources: Set<String>?) async throws -> UsageReport {
        try await fetchUsage(agents: agents, historyHours: historyHours)
    }
    public func fileChanges(_ paths: Set<String>?) async {}
    public func sourceChecks() async -> [String: [Date]] { [:] }
    public func accountChecks(since: [String: Date], now: Date) async -> [String: Date] { [:] }
    public var seesLocalWork: Bool { true }
}

/// The collection pipeline's cadence. Reads never run in parallel: one account step or one source read at a time.
public enum UsageRefresh {
    /// The fallback read of every local source, and the quota and balance readings of a source that cannot tell its
    /// own work from quiet, or whose windows say nothing about when they change.
    public static let accountInterval: TimeInterval = 300
    /// Quota and balance readings while a turn is running, which is when the windows move fastest.
    public static let runningAccountInterval: TimeInterval = 60
    /// Quota and balance readings while a session is live between turns.
    public static let liveAccountInterval: TimeInterval = 180
    /// A provider never repeats an account request sooner than this, whoever asks.
    public static let accountRequestSpacing: TimeInterval = 60
    /// A source that cannot name its directories is read this often.
    public static let pollInterval: TimeInterval = 5
    /// Local logs while the first index is still being built.
    public static let indexingInterval: TimeInterval = 2
    /// Local reads start at most this often; changes arriving sooner are read together.
    public static let readSpacing: TimeInterval = 2
    /// How long a quiet log keeps a session that never said what its turn is doing. A source that reports turn states
    /// ignores it: there, only the turn decides.
    public static let liveThreshold: TimeInterval = 120
    /// The far side of a running turn's silence. One tool call can keep a log quiet for minutes, so silence alone does
    /// not end a turn; a turn this quiet was abandoned — its client was killed, or its logs stopped reaching this Mac.
    /// TODO: drop this once every provider reports a client heartbeat and can say so outright.
    public static let abandonedTurnTimeout: TimeInterval = 30 * 60
    /// Account steps run back to back for at most this long before local logs get their turn.
    static let accountStepBudget: TimeInterval = 1
    /// A running turn counts as current work while its latest source observation is this recent.
    static let activeTurnFreshness: TimeInterval = 300
}

public struct UsageProviderError: Error, Hashable, Sendable, LocalizedError {
    public let message: String

    public init(_ message: String) {
        self.message = message
    }

    public var errorDescription: String? { message }
}
