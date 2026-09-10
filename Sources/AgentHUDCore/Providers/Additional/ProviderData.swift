import AgentHUDSupport
import Foundation

/// The additional integrations share reporting, while each owns its protocol and parser.
public enum AdditionalSource: String, CaseIterable, Sendable {
    case antigravity, cursor, grok

    public var vendor: String {
        switch self {
        case .antigravity: "Antigravity"
        case .cursor: "Cursor"
        case .grok: "Grok"
        }
    }

    public var detail: String {
        switch self {
        case .antigravity: L10n.text("本地服务额度与会话用量", "Local server quota and session usage")
        case .cursor: L10n.text("账户额度与跨设备用量", "Account quota and usage across devices")
        case .grok: L10n.text("Grok CLI 额度与本地会话", "Grok CLI quota and local sessions")
        }
    }

    public func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        let paths: [String]
        switch self {
        case .antigravity: paths = [".gemini/antigravity", ".gemini/antigravity-cli", "Applications/Antigravity.app"]
        case .cursor: paths = ["Library/Application Support/Cursor/User/globalStorage/state.vscdb", "Applications/Cursor.app"]
        case .grok: paths = [".grok"]
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
    var origin: TranscriptSession.UsageEvent.Origin? = nil

    func usage(source: AdditionalSource) -> TranscriptSession.UsageEvent {
        .init(timestamp: timestamp, agentId: "\(source.rawValue)-model:\(model)", tokensIn: input,
              tokensOut: output, cacheReadTokens: cacheRead, eventID: "\(source.rawValue):\(id)", origin: origin)
    }
}

struct ProviderSessions: Sendable {
    var sessions: [ProviderSession] = []
    var notice: String? = nil
    var indexing: IndexProgress? = nil
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
