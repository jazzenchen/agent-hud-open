import AgentHUDSupport
import Foundation

/// Qwen Code chats: `<runtime>/projects/<project>/chats/<session>.jsonl`, where the runtime folder is
/// `QWEN_RUNTIME_DIR`, else `QWEN_HOME`, else `~/.qwen`.
///
/// Usage comes from the `qwen-code.api_response` telemetry line Qwen writes for every completed model call, which is
/// also what Qwen rebuilds its own statistics from. Assistant lines carry the same counts for main-agent calls only,
/// so sub-agent calls would be missed there; sub-agent transcripts are not read, because the parent's telemetry
/// already covers them.
enum QwenSessions: LocalSessionLayout {
    static let installPaths = [".qwen/projects"]

    /// Qwen Code's configuration folder: `QWEN_HOME`, else `~/.qwen`.
    static func home(_ home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["QWEN_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appendingPathComponent(".qwen")
    }

    static func roots(home: URL, environment: [String: String]) -> [URL] {
        let runtime = environment["QWEN_RUNTIME_DIR"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
            ?? self.home(home, environment: environment)
        return [runtime.appendingPathComponent("projects")]
    }

    /// Session transcripts only: the serve ledger and a streaming sub-agent copy are other records.
    static func accepts(_ url: URL) -> Bool {
        url.pathExtension == "jsonl" && !url.lastPathComponent.hasSuffix(".ledger.jsonl")
            && url.deletingLastPathComponent().pathComponents.contains("chats")
    }

    static func skips(_ url: URL) -> Bool { url.lastPathComponent == "subagents" }

    static func read(_ url: URL) throws -> ProviderSessions {
        let stem = url.deletingPathExtension().lastPathComponent
        var sessions: [String: ProviderSession] = [:], events: [String: [String: ProviderEvent]] = [:]
        var titled: Set<String> = [], incomplete = false
        try ProviderFiles.lines(url) { json, line in
            let raw = nonEmpty(json["sessionId"]) ?? stem
            let id = "qwen:\(raw)", date = ProviderDate.iso(json["timestamp"].stringValue)
            var session = sessions[id] ?? ProviderSession(id: id, title: "Qwen · \(raw.prefix(8))", path: url.path, client: "Qwen Code")
            if session.workspace == nil, let cwd = nonEmpty(json["cwd"]) {
                session.workspace = cwd
                if !titled.contains(id) { session.title = URL(fileURLWithPath: cwd).lastPathComponent }
            }
            if let date { session.startedAt = min(session.startedAt ?? date, date); session.lastActivity = max(session.lastActivity ?? date, date) }
            // A title the user or Qwen gave the session wins; else the first prompt typed into it.
            if json["type"].stringValue == "system", json["subtype"].stringValue == "custom_title",
               let title = nonEmpty(json["systemPayload"]["customTitle"]).flatMap(SessionTitle.from) {
                session.title = title; titled.insert(id)
            } else if !titled.contains(id), json["type"].stringValue == "user", json["subtype"] == .null,
                      let text = json["message"]["parts"].arrayValue?.compactMap({ $0["text"].stringValue }).first,
                      let title = SessionTitle.from(text) {
                session.title = title; titled.insert(id)
            }
            sessions[id] = session

            // `/branch` copies the whole chain into the new session; the copies were counted where they happened.
            guard json["forkedFrom"] == .null, json["type"].stringValue == "system", json["subtype"].stringValue == "ui_telemetry" else { return }
            let event = json["systemPayload"]["uiEvent"]
            guard event["event.name"].stringValue == "qwen-code.api_response" else { return }
            let counted: (input: Int, output: Int, cache: Int)?
            do { counted = try tokens(event) } catch { incomplete = true; return }
            guard let tokens = counted else { return }
            guard let time = ProviderDate.iso(event["event.timestamp"].stringValue) ?? date else { incomplete = true; return }
            let key = nonEmpty(json["uuid"]) ?? "\(raw):line-\(line)"
            events[id, default: [:]][key] = ProviderEvent(id: key, model: nonEmpty(event["model"]) ?? "Unknown", timestamp: time,
                                                          input: tokens.input, output: tokens.output, cacheRead: tokens.cache)
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { key in
            sessions[key].map { var item = $0; item.events = (events[key] ?? [:]).values.sorted { ($0.timestamp, $0.id) < ($1.timestamp, $1.id) }; return item }
        }, notice: incomplete ? L10n.text("部分 Qwen 记录缺少时间或计数无法核对，未计入统计",
                                          "Some Qwen records lack a time or have inconsistent counts and were excluded") : nil)
    }

    /// Qwen reports the prompt with cache reads inside it and, for every OpenAI-compatible and Anthropic backend, the
    /// output with reasoning inside it. Only Gemini's own API counts thoughts beside the output, which the total says.
    static func tokens(_ event: ProviderJSON) throws -> (input: Int, output: Int, cache: Int)? {
        let prompt = try event["input_token_count"].optionalCounter(), output = try event["output_token_count"].optionalCounter()
        let cache = try event["cached_content_token_count"].optionalCounter(), thoughts = try event["thoughts_token_count"].optionalCounter()
        guard cache <= prompt else { throw ProviderFailure.format }
        let total = event["total_token_count"] == .null ? nil : try event["total_token_count"].optionalCounter()
        let separate = try thoughts > 0 && total == TokenCount.sum(prompt, output, thoughts)
        let out = separate ? try TokenCount.sum(output, thoughts) : output
        return try TokenCount.sum(prompt, out) > 0 ? (prompt - cache, out, cache) : nil
    }

    private static func nonEmpty(_ value: ProviderJSON) -> String? {
        value.stringValue.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
    }
}
