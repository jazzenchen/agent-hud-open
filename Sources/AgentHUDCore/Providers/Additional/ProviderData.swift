import AgentHUDSupport
import Foundation

/// The additional integrations share reporting, while each owns its protocol and parser.
public enum AdditionalSource: String, CaseIterable, Sendable {
    case antigravity, cursor, grok, copilot, openclaw, hermes, zcode, codebuddy, workbuddy, qwen

    public var vendor: String {
        switch self {
        case .antigravity: "Antigravity"
        case .cursor: "Cursor"
        case .grok: "Grok"
        case .copilot: "GitHub Copilot"
        case .openclaw: "OpenClaw"
        case .hermes: "Hermes"
        case .zcode: "ZCode"
        case .codebuddy: "CodeBuddy"
        case .workbuddy: "WorkBuddy"
        case .qwen: "Qwen"
        }
    }

    public var detail: String {
        switch self {
        case .antigravity: L10n.text("本地服务额度与会话用量", "Local server quota and session usage")
        case .cursor: L10n.text("账户额度与跨设备用量", "Account quota and usage across devices")
        case .grok: L10n.text("Grok CLI 额度与本地会话", "Grok CLI quota and local sessions")
        case .copilot: L10n.text("Copilot CLI 额度与本地会话", "Copilot CLI quota and local sessions")
        case .openclaw: L10n.text("OpenClaw 本地会话与用量", "OpenClaw local sessions and usage")
        case .hermes: L10n.text("Hermes Agent 本地会话用量", "Hermes Agent local session usage")
        case .zcode: L10n.text("ZCode 本地会话与用量", "ZCode local sessions and usage")
        case .codebuddy: L10n.text("CodeBuddy Code 本地会话与用量", "CodeBuddy Code local sessions and usage")
        case .workbuddy: L10n.text("WorkBuddy 本地会话与用量", "WorkBuddy local sessions and usage")
        case .qwen: L10n.text("Qwen Code 本地会话与用量", "Qwen Code local sessions and usage")
        }
    }

    /// Database readers return only this much recent history, however long the requested window.
    var readerWindow: TimeInterval? {
        switch self {
        case .openclaw, .hermes, .zcode: 8 * 86400
        default: nil
        }
    }

    public func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        let paths: [String]
        switch self {
        case .cursor: paths = ["Library/Application Support/Cursor/User/globalStorage/state.vscdb", "Applications/Cursor.app"]
        case .antigravity, .grok: paths = layout?.installPaths ?? []
        // A same-named desktop app is not the CLI these clients read, and it must not trigger hook installation.
        default: return layout?.installPaths.contains { FileManager.default.fileExists(atPath: home.appendingPathComponent($0).path) } ?? false
        }
        return paths.contains { FileManager.default.fileExists(atPath: home.appendingPathComponent($0).path) }
            || FileManager.default.fileExists(atPath: "/Applications/\(vendor).app")
    }
}

struct ProviderQuota: Sendable {
    struct Window: Sendable {
        let id: String
        let label: String
        let remaining: Double
        var reset: Date? = nil
        var duration: TimeInterval? = nil
    }
    var windows: [Window] = []
    var plan: String? = nil
    var notice: String? = nil
    /// The signed-in account the quota belongs to; nil when the service answered without naming it.
    var account: ProviderAccount? = nil
    /// An email or name from the same response or login record, shown to this Mac's user.
    var label: String? = nil
    /// The user withdrew access: the vendor's accounts, rows and quota history are forgotten, not kept as last readings.
    var forgetAccounts = false

    /// Whether the service answered for a signed-in account, as opposed to finding no client.
    var isSignedIn: Bool { account != nil || !windows.isEmpty }

    func resolvedAccount(_ source: AdditionalSource) -> ProviderAccount {
        account ?? .unresolved(provider: source.vendor, home: "")
    }

    /// Window ids scoped to the account: `account:<hash>/cursor:team`.
    func scopedWindows(_ source: AdditionalSource) -> [Window] {
        let account = resolvedAccount(source)
        return windows.map { Window(id: account.windowID($0.id), label: $0.label, remaining: $0.remaining, reset: $0.reset, duration: $0.duration) }
    }
}

struct ProviderSession: Sendable {
    let id: String
    var title: String
    var workspace: String? = nil
    var path: String? = nil
    var client: String
    var events: [ProviderEvent] = []
    var startedAt: Date?
    var lastActivity: Date?
    var turns: [SessionTurn] = []
    var completions: [SessionCompletion] = []
    var accountWide = false
}

struct ProviderEvent: Hashable, Sendable {
    let id: String
    let model: String
    let timestamp: Date
    let input: Int
    let output: Int
    var cacheRead: Int = 0
    /// The part of `input` written to the cache and the part of `output` spent reasoning.
    var cacheWrite: Int = 0
    var reasoning: Int = 0
    var origin: UsageEvent.Origin? = nil

    func usage(source: AdditionalSource) -> UsageEvent {
        .init(timestamp: timestamp, agentId: "\(source.rawValue)-model:\(model)", tokensIn: input, tokensOut: output, cacheReadTokens: cacheRead,
              cacheWriteTokens: cacheWrite, reasoningTokens: reasoning, eventID: "\(source.rawValue):\(id)", origin: origin)
    }
}

struct ProviderSessions: Sendable {
    var sessions: [ProviderSession] = []
    var notice: String? = nil
    var indexing: IndexProgress? = nil
    /// Changes whenever the reader's results change; nil when the reader cannot tell.
    var revision: Int? = nil
    /// The files the sessions come from; nil for a reader of account records.
    var files: ListedFiles? = nil
}

// The existing JSON value representation keeps parsed data Sendable without passing Foundation Any graphs.
typealias ProviderJSON = JSONValue

extension JSONValue {
    subscript(_ key: String) -> JSONValue { objectValue?[key] ?? .null }
    var objectValue: [String: JSONValue]? { if case .object(let value) = self { return value }; return nil }
    var arrayValue: [JSONValue]? { if case .array(let value) = self { return value }; return nil }
    var stringValue: String? { if case .string(let value) = self { return value }; return nil }
    var boolValue: Bool? { if case .bool(let value) = self { return value }; return nil }
    var numberValue: Double? {
        switch self {
        case .integer(let value): return Double(value)
        case .number(let value) where value.isFinite: return value
        default: return nil
        }
    }
    var countValue: Int? {
        switch self {
        case .integer(let value) where value >= 0: return Int(exactly: value)
        case .number(let value) where value.isFinite && value >= 0 && value.rounded() == value: return Int(exactly: value)
        default: return nil
        }
    }
    func optionalCounter() throws -> Int {
        if self == .null { return 0 }
        guard let count = countValue else { throw ProviderFailure.format }
        return count
    }
    static func read(_ data: Data) throws -> Self { try JSONDecoder().decode(Self.self, from: data) }
}

enum ProviderDate {
    static func iso(_ text: String?) -> Date? {
        guard let text else { return nil }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let value = formatter.date(from: text) { return value }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: text)
    }
    static func milliseconds(_ value: ProviderJSON) -> Date? {
        let number = value.numberValue ?? value.stringValue.flatMap(Double.init)
        guard let number, number.isFinite, number > 0, number <= 253402300799999 else { return nil }
        return Date(timeIntervalSince1970: number / 1000)
    }
    static func period(start: Date?, end: Date?) -> TimeInterval? {
        guard let start, let end, end > start else { return nil }
        return end.timeIntervalSince(start)
    }
}

enum ProviderFailure {
    static var format: UsageProviderError { .init(L10n.text("用量数据格式无法识别，请更新后重试", "Usage data has an unsupported format; update and retry")) }
    static var local: UsageProviderError { .init(L10n.text("部分本地会话无法读取，用量可能不完整", "Some local sessions could not be read; usage may be incomplete")) }
    static var limit: UsageProviderError { .init(L10n.text("会话超过本轮读取上限，用量尚不完整", "Session read limit reached; usage is incomplete")) }
    static func login(_ vendor: String) -> UsageProviderError {
        .init(L10n.text("请先登录 \(vendor)，再刷新额度", "Sign in to \(vendor), then refresh quota"))
    }
}
