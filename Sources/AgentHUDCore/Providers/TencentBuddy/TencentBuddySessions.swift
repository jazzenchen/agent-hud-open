import AgentHUDSupport
import Foundation

// CodeBuddy / WorkBuddy transcript fields, token layouts and request identities follow Tokscale tencent_buddy.rs (MIT).
enum TencentBuddySessions {
    static func read(_ url: URL, source: AdditionalSource) throws -> ProviderSessions {
        let folder = url.deletingLastPathComponent(), stem = url.deletingPathExtension().lastPathComponent
        // `<session>/subagents/*.jsonl` belong to their parent session.
        let parent = folder.lastPathComponent == "subagents" ? folder.deletingLastPathComponent().lastPathComponent : nil
        let client = source == .codebuddy ? "CodeBuddy Code" : source.vendor
        var sessions: [String: ProviderSession] = [:], events: [String: [String: ProviderEvent]] = [:], incomplete = false
        try ProviderFiles.lines(url) { json, line in
            let raw = parent ?? json["sessionId"].stringValue.flatMap { $0.isEmpty ? nil : $0 } ?? stem
            let id = "\(source.rawValue):\(raw)", date = ProviderDate.milliseconds(json["timestamp"])
            var session = sessions[id] ?? ProviderSession(id: id, title: "\(source.vendor) · \(raw.prefix(8))", path: url.path, client: client)
            if session.workspace == nil, let cwd = json["cwd"].stringValue, !cwd.isEmpty {
                session.workspace = cwd
                session.title = URL(fileURLWithPath: cwd).lastPathComponent
            }
            if let date { session.startedAt = min(session.startedAt ?? date, date); session.lastActivity = max(session.lastActivity ?? date, date) }
            sessions[id] = session
            let kind = json["type"].stringValue, provider = json["providerData"]
            guard kind == "function_call" || (kind == "message" && json["role"].stringValue == "assistant"),
                  json["status"] == .null || json["status"].stringValue == "completed",
                  let usage = [json["message"]["usage"], provider["usage"], provider["rawUsage"]].first(where: { $0.objectValue != nil }) else { return }
            let counted: (input: Int, output: Int, cache: Int)?
            do { counted = try self.tokens(usage) } catch { incomplete = true; return }
            guard let tokens = counted else { return }
            guard let date else { incomplete = true; return }
            // One response can be written on both its message and its function call line.
            let key = nonEmpty(provider["messageId"]).map { "message:\($0)" } ?? nonEmpty(provider["traceId"]).map { "trace:\($0)" }
                ?? nonEmpty(json["id"]).map { "\(raw):item:\($0)" } ?? "\(raw):\(stem):line-\(line)"
            let event = ProviderEvent(id: key, model: model(json) ?? "Unknown", timestamp: date, input: tokens.input, output: tokens.output, cacheRead: tokens.cache)
            if let previous = events[id]?[key], previous.input + previous.output + previous.cacheRead > event.input + event.output + event.cacheRead { return }
            events[id, default: [:]][key] = event
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { key in
            sessions[key].map { var item = $0; item.events = (events[key] ?? [:]).values.sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }; return item }
        }, notice: incomplete ? L10n.text("部分 \(source.vendor) 记录缺少时间或计数无法核对，未计入统计",
                                          "Some \(source.vendor) records lack a time or have inconsistent counts and were excluded") : nil)
    }

    /// A reported total equal to input + output, or cached tokens nested in the input details, proves that input holds cache
    /// reads and output holds reasoning; otherwise the counts are additive. In keeps cache writes; Cache is cache reads only.
    static func tokens(_ usage: ProviderJSON) throws -> (input: Int, output: Int, cache: Int)? {
        func counts(_ keys: [String]) throws -> [Int] { try keys.compactMap { usage[$0] == .null ? nil : try usage[$0].optionalCounter() } }
        func positive(_ keys: [String]) throws -> Int? { let values = try counts(keys); return values.first { $0 > 0 } ?? values.first }
        func nested(_ keys: [String], _ field: String) throws -> Int? {
            guard let key = keys.first(where: { usage[$0] != .null }) else { return nil }
            let values = try (usage[key].arrayValue ?? [usage[key]]).compactMap { $0[field] == .null ? nil : try $0[field].optionalCounter() }
            return values.isEmpty ? nil : try values.reduce(0) { try TokenCount.sum($0, $1) }
        }
        let listed = try positive(["cache_read_input_tokens", "cacheReadInputTokens", "cacheTokens", "prompt_cache_hit_tokens", "cached_tokens"])
        let detailed = try nested(["inputTokensDetails", "input_tokens_details", "prompt_tokens_details"], "cached_tokens")
        let cache = listed ?? detailed ?? 0
        let input = try counts(["input_tokens", "inputTokens", "prompt_tokens"]).first ?? 0
        let output = try counts(["output_tokens", "outputTokens", "completion_tokens"]).first ?? 0
        let write = try positive(["cache_creation_input_tokens", "cacheCreationInputTokens", "cachedWriteTokens", "prompt_cache_write_tokens"]) ?? 0
        let reasoning = try counts(["completion_thinking_tokens", "completionThinkingTokens", "reasoningTokens"]).first ?? 0
        let total = try counts(["total_tokens", "totalTokens"]).first
        let inclusive = try listed == nil && detailed != nil || total == TokenCount.sum(input, output)
        let fresh: Int
        if let miss = try counts(["cachedMissTokens", "cacheMissTokens"]).first {
            fresh = inclusive ? miss : try TokenCount.sum(miss, write)
        } else if inclusive {
            guard cache <= input else { throw ProviderFailure.format }
            fresh = input - cache
        } else { fresh = try TokenCount.sum(input, write) }
        let out = inclusive ? output : try TokenCount.sum(output, reasoning)
        return try TokenCount.sum(fresh, out, cache) > 0 ? (fresh, out, cache) : nil
    }

    static func model(_ json: ProviderJSON) -> String? {
        // Written out rather than as a lazy sequence over the three values: Swift 6.4 fails SIL ownership
        // verification on a borrowed JSONValue inside one, and brings the optimizer down with it.
        nonEmpty(json["providerData"]["model"])
            ?? nonEmpty(json["providerData"]["requestModelId"])
            ?? nonEmpty(json["message"]["model"])
    }

    /// The final completed assistant message of the latest turn, searched in at most the last 1 MiB of a transcript.
    static func lastAssistantMessage(_ url: URL, session: String) -> (id: String, model: String?)? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        guard let size = try? handle.seekToEnd() else { return nil }
        let start = size > 1 << 20 ? size - (1 << 20) : 0
        guard (try? handle.seek(toOffset: start)) != nil, let data = try? handle.readToEnd() else { return nil }
        var lines = data.split(separator: 10)
        if start > 0, !lines.isEmpty { lines.removeFirst() }
        for line in lines.reversed() {
            guard let json = try? ProviderJSON.read(Data(line)), json["type"].stringValue == "message",
                  json["sessionId"].stringValue.map({ $0 == session }) != false else { continue }
            if json["role"].stringValue == "user" { return nil }
            guard json["role"].stringValue == "assistant", json["status"].stringValue == "completed" else { continue }
            return nonEmpty(json["providerData"]["messageId"]).map { ($0, model(json)) }
        }
        return nil
    }

    /// Sub-agent transcripts share the parent session; keep the parent's metadata and the overall time span.
    static func merge(_ sessions: [ProviderSession]) -> [ProviderSession] {
        let groups = Dictionary(grouping: sessions, by: \.id)
        return sessions.map { item in
            guard let group = groups[item.id], group.count > 1 else { return item }
            let main = group.first { $0.path?.contains("/subagents/") == false && $0.workspace != nil } ?? group.first { $0.workspace != nil } ?? item
            var item = item
            item.title = main.title; item.workspace = main.workspace; item.path = main.path
            item.startedAt = group.compactMap(\.startedAt).min(); item.lastActivity = group.compactMap(\.lastActivity).max()
            return item
        }
    }

    private static func nonEmpty(_ value: ProviderJSON) -> String? {
        value.stringValue.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
    }
}

enum CodeBuddySessions: LocalSessionLayout {
    static let installPaths = [".codebuddy/projects"]
    /// CodeBuddy Code's configuration and data folder: `CODEBUDDY_CONFIG_DIR`, else `~/.codebuddy`.
    static func home(_ home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["CODEBUDDY_CONFIG_DIR"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appendingPathComponent(".codebuddy")
    }
    static func roots(home: URL, environment: [String: String]) -> [URL] { [self.home(home, environment: environment).appendingPathComponent("projects")] }
    static func accepts(_ url: URL) -> Bool { url.pathExtension == "jsonl" }
    static func skips(_ url: URL) -> Bool { url.lastPathComponent == "tool-results" }
    static func read(_ url: URL) throws -> ProviderSessions { try TencentBuddySessions.read(url, source: .codebuddy) }
    static func merge(_ sessions: [ProviderSession]) -> [ProviderSession] { TencentBuddySessions.merge(sessions) }
}

enum WorkBuddySessions: LocalSessionLayout {
    static let installPaths = [".workbuddy/projects"]
    static func roots(home: URL, environment: [String: String]) -> [URL] { [home.appendingPathComponent(".workbuddy/projects")] }
    static func accepts(_ url: URL) -> Bool { url.pathExtension == "jsonl" }
    static func skips(_ url: URL) -> Bool { url.lastPathComponent == "tool-results" }
    static func read(_ url: URL) throws -> ProviderSessions { try TencentBuddySessions.read(url, source: .workbuddy) }
    static func merge(_ sessions: [ProviderSession]) -> [ProviderSession] { TencentBuddySessions.merge(sessions) }
}
