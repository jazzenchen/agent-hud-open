import AgentHUDSupport
import Foundation

/// One model call of a turn as the usage ledger recorded it, with the tools its log says it asked for.
public struct TurnCall: Hashable, Sendable {
    public let timestamp: Date
    public let agentId: String
    public let tokens: SessionUsage.Tokens
    /// The session's own log rather than a sub-agent's.
    public let own: Bool
    /// The log the call was read from and the source that read it.
    public let log: String
    public let source: String
    /// The call's prompt with cache reads, for calls in the session's log that records every call.
    public let context: Int?
    /// Prompt tokens the call sent again because the cache no longer held them.
    public let recached: Int?
    /// What the call would cost at the vendors' API list prices, in the unit of the session's `listCost`.
    public let listCost: Decimal?
    /// The tools the call asked for, in order; empty when it asked for none or its log names none.
    public var tools: [String] = []
}

/// Reads which tools each call of a Claude Code or Codex log asked for, keyed by the millisecond the ledger records the
/// call at: a Claude Code response's first line, a Codex token count that moved the totals.
enum CallTools {
    /// Sub-agents' logs are read only while a turn has at most this many; a turn that fanned out wider shows its own tools.
    static let subagentLogLimit = 16
    /// How far past a turn's last call its log is read, for the lines that finish that call.
    private static let tail: TimeInterval = 600

    /// The calls with the tools their logs name.
    static func attach(to calls: [TurnCall]) -> [TurnCall] {
        let byLog = Dictionary(grouping: calls.indices) { calls[$0].log }
        let subagentLogs = Set(calls.lazy.filter { !$0.own }.map(\.log)).count
        var result = calls
        for (log, indices) in byLog {
            let first = calls[indices[0]]
            guard first.own || subagentLogs <= subagentLogLimit else { continue }
            let times = indices.map { calls[$0].timestamp }
            let tools = read(log: log, source: first.source, from: times.min()!, through: times.max()!)
            for index in indices { result[index].tools = tools[RecordCoding.milliseconds(calls[index].timestamp)] ?? [] }
        }
        return result
    }

    static func read(log path: String, source: String, from start: Date, through end: Date) -> [Int64: [String]] {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .alwaysMapped) else { return [:] }
        switch source {
        case "claude": return claude(data, from: start, through: end)
        case "codex": return codex(data, from: start, through: end)
        default: return [:]
        }
    }

    /// Claude Code writes a response one content block per line, all under the message's id; the first line with usage
    /// is the call.
    static func claude(_ data: Data, from start: Date, through end: Date) -> [Int64: [String]] {
        let window = Window(start: start.addingTimeInterval(-1), end: end.addingTimeInterval(tail))
        var calls: [String: (at: Int64?, tools: [String])] = [:]
        lines(data) { line in
            guard line.range(of: assistant) != nil, window.contains(line, last: true),
                  let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  let timestamp = (object["timestamp"] as? String).flatMap(DateParsing.iso8601),
                  let message = object["message"] as? [String: Any],
                  let key = (message["id"] as? String).map({ "m:" + $0 }) ?? (object["requestId"] as? String).map({ "r:" + $0 })
            else { return }
            let usage = message["usage"] as? [String: Any] ?? [:]
            let counted = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens"]
                .reduce(0) { $0 + (usage[$1] as? Int ?? 0) }
            let tools = (message["content"] as? [[String: Any]] ?? []).compactMap { block in
                block["type"] as? String == "tool_use" ? block["name"] as? String : nil
            }
            var call = calls[key] ?? (nil, [])
            if call.at == nil, counted > 0 { call.at = RecordCoding.milliseconds(timestamp) }
            call.tools += tools
            calls[key] = call
        }
        return Dictionary(calls.values.compactMap { call in call.at.map { ($0, call.tools) } }) { $0 + $1 }
    }

    /// Codex writes a call's tool requests before the token count that closes it; a count that does not move the totals
    /// closes nothing.
    static func codex(_ data: Data, from start: Date, through end: Date) -> [Int64: [String]] {
        let window = Window(start: start.addingTimeInterval(-tail), end: end.addingTimeInterval(1))
        var result: [Int64: [String]] = [:], pending: [String] = [], totals: Int?
        lines(data) { line in
            guard line.range(of: tokenCount) != nil || codexRequests.contains(where: { line.range(of: $0) != nil }),
                  window.contains(line, last: false),
                  let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  let payload = object["payload"] as? [String: Any], let kind = payload["type"] as? String else { return }
            switch kind {
            case "function_call", "custom_tool_call": if let name = payload["name"] as? String { pending.append(name) }
            case "local_shell_call": pending.append("shell")
            case "web_search_call": pending.append("web_search")
            case "token_count":
                guard let usage = (payload["info"] as? [String: Any])?["total_token_usage"] as? [String: Any],
                      let timestamp = (object["timestamp"] as? String).flatMap(ISO8601Fast.parse) else { return }
                let total = (usage["input_tokens"] as? Int ?? 0) + (usage["output_tokens"] as? Int ?? 0)
                defer { totals = total }
                guard total != totals else { return }
                result[RecordCoding.milliseconds(timestamp)] = pending
                pending = []
            default: break
            }
        }
        return result
    }

    private static let assistant = Data(#""type":"assistant""#.utf8)
    private static let tokenCount = Data(#""type":"token_count""#.utf8)
    private static let codexRequests = [#""type":"function_call""#, #""type":"custom_tool_call""#, #""type":"local_shell_call""#,
                                        #""type":"web_search_call""#].map { Data($0.utf8) }

    /// Seconds-precision bounds, compared with a line's own ISO 8601 time before the line is decoded.
    private struct Window {
        static let key = Data(#""timestamp":""#.utf8)
        let lower: String, upper: String

        init(start: Date, end: Date) {
            let format = ISO8601DateFormatter()
            lower = String(format.string(from: start).prefix(19)); upper = String(format.string(from: end).prefix(19))
        }

        /// Whether the line's time, its first or last `"timestamp"` key, lies in the window; a line without one is let
        /// through for its decoded time to decide.
        func contains(_ line: Data, last: Bool) -> Bool {
            guard let key = line.range(of: Self.key, options: last ? .backwards : []),
                  line.distance(from: key.upperBound, to: line.endIndex) >= 19 else { return true }
            let stamp = String(decoding: line[key.upperBound..<line.index(key.upperBound, offsetBy: 19)], as: UTF8.self)
            return stamp >= lower && stamp <= upper
        }
    }

    private static func lines(_ data: Data, _ body: (Data) -> Void) {
        data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            guard let base = buffer.baseAddress else { return }
            var offset = 0
            while offset < buffer.count {
                let found = memchr(base + offset, 0x0A, buffer.count - offset)
                let end = found.map { base.distance(to: UnsafeRawPointer($0)) } ?? buffer.count
                if end > offset {
                    body(Data(bytesNoCopy: UnsafeMutableRawPointer(mutating: base + offset), count: end - offset, deallocator: .none))
                }
                offset = end + 1
            }
        }
    }
}
