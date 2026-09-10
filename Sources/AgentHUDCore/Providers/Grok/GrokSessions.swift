import AgentHUDSupport
import Foundation

// Recorded Grok schemas and source precedence follow Tokscale grok.rs (MIT).
enum GrokSessions {
    static func read(_ url: URL) throws -> ProviderSessions {
        url.lastPathComponent == "unified.jsonl" ? try unified(url) : try updates(url)
    }

    static func updates(_ url: URL) throws -> ProviderSessions {
        let directory = url.deletingLastPathComponent()
        let rawID = directory.lastPathComponent, id = "grok:\(rawID)"
        let workspace = directory.deletingLastPathComponent().lastPathComponent.removingPercentEncoding
        let summary = (try? ProviderFiles.json(directory.appendingPathComponent("summary.json"))) ?? .null
        var model = "Unknown"
        let title = summary["title"].stringValue ?? workspace.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Grok CLI"
        var session = ProviderSession(id: id, title: title, workspace: workspace, path: url.path, client: "Grok CLI")
        var seen = Set<String>(), turnID: String?, turnStart: Date?, incomplete = false, hasContextOnly = false
        try ProviderFiles.lines(url) { json, line in
            guard let method = json["method"].stringValue,
                  method == "session/update" || method == "_x.ai/session/update" else { return }
            let params = json["params"], update = params["update"], meta = params["_meta"]
            guard params["sessionId"].stringValue == rawID else { throw ProviderFailure.format }
            guard let date = ProviderDate.milliseconds(meta["agentTimestampMs"]) else { return }
            let eventID = meta["eventId"].stringValue ?? "line-\(line)"
            guard seen.insert(eventID).inserted else { return }
            session.startedAt = min(session.startedAt ?? date, date)
            session.lastActivity = max(session.lastActivity ?? date, date)
            if let value = update["_meta"]["modelId"].stringValue ?? meta["modelId"].stringValue { model = value }
            let kind = update["sessionUpdate"].stringValue
            if kind == "user_message_chunk", turnID == nil {
                turnID = meta["promptId"].stringValue ?? eventID
                turnStart = ProviderDate.milliseconds(meta["turnStartMs"]) ?? date
            }
            if let turnID, kind == "user_message_chunk" || kind == "agent_message_chunk" || kind == "agent_thought_chunk" || kind == "tool_call" || kind == "tool_call_update" {
                session.turns.removeAll { $0.turnID == turnID }
                session.turns.append(.init(provider: "Grok", sessionID: id, turnID: turnID, state: .running,
                    startedAtMs: turnStart.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(date)))
            }
            guard kind == "turn_completed" else {
                if meta["totalTokens"].countValue ?? 0 > 0 { hasContextOnly = true }
                return
            }
            let usage = update["usage"]
            let usedModels = usage["modelUsage"].objectValue?.keys.sorted() ?? []
            if usedModels.count == 1 { model = usedModels[0] }
            if let input = usage["inputTokens"].countValue, let output = usage["outputTokens"].countValue {
                let cache = try (usage["cachedReadTokens"] == .null ? usage["cacheReadTokens"] : usage["cachedReadTokens"]).optionalCounter()
                guard cache <= input else { throw ProviderFailure.format }
                session.events.append(.init(id: "\(id):\(eventID)", model: model, timestamp: date, input: input - cache, output: output, cacheRead: cache,
                    origin: .init(group: id, priority: 1)))
            } else { incomplete = true }
            let completedID = update["prompt_id"].stringValue ?? turnID ?? eventID
            if let turnID { session.turns.removeAll { $0.turnID == turnID } }
            session.turns.removeAll { $0.turnID == completedID }
            let succeeded = update["stop_reason"].stringValue == "end_turn"
            session.turns.append(.init(provider: "Grok", sessionID: id, turnID: completedID, state: succeeded ? .completed : .ended,
                startedAtMs: turnStart.map(RecordCoding.milliseconds), observedAtMs: RecordCoding.milliseconds(date)))
            if succeeded {
                session.completions.append(.init(sessionID: id, vendor: "Grok", turnID: completedID, task: title,
                    model: model, startedAt: turnStart, completedAt: date))
            }
            turnID = nil; turnStart = nil
        }
        return ProviderSessions(sessions: [session], notice: incomplete || (hasContextOnly && session.events.isEmpty)
            ? L10n.text("部分 Grok 旧会话只有上下文计数，无法还原实际 Token 消耗", "Some older Grok sessions only report context size, not token consumption") : nil)
    }

    static func unified(_ url: URL) throws -> ProviderSessions {
        var sessions: [String: ProviderSession] = [:], models: [String: String] = [:], seen = Set<String>()
        var generations: [Int: Int] = [:], processModels: [String: String] = [:], processSessions: [String: Set<String>] = [:]
        var pendingModels: [String: (process: String, model: String)] = [:]
        try ProviderFiles.lines(url) { json, _ in
            let pid = json["pid"].countValue
            if json["msg"].stringValue == "AuthManager::new", let pid { generations[pid, default: 0] += 1; return }
            let process = pid.map { "\($0):\(generations[$0, default: 0])" }
            let context = json["ctx"]
            let changedModel: String?
            switch json["msg"].stringValue {
            case "model changed": changedModel = context["model"].stringValue
            case "model catalog: notifying clients": changedModel = context["current_model_id"].stringValue
            case "backend_search: model switch": changedModel = context["new_model"].stringValue
            default: changedModel = nil
            }
            if json["sid"].stringValue == nil, let process, let changedModel { processModels[process] = changedModel; return }
            guard let rawID = json["sid"].stringValue, !rawID.isEmpty else { return }
            let id = "grok:\(rawID)"
            // A model is scoped to the exact session and process that reported it; no parent-model guessing.
            let scope = id + ":" + (process ?? "")
            switch json["msg"].stringValue {
            case "model changed": models[scope] = context["model"].stringValue; return
            case "model catalog: notifying clients": models[scope] = context["current_model_id"].stringValue; return
            case "backend_search: model switch": models[scope] = context["new_model"].stringValue; return
            case "shell.turn.inference_done": break
            default: return
            }
            guard let date = ProviderDate.iso(json["ts"].stringValue) ?? ProviderDate.milliseconds(json["ts"]),
                  let input = context["prompt_tokens"].countValue, let output = context["completion_tokens"].countValue else { throw ProviderFailure.format }
            let cache = try context["cached_prompt_tokens"].optionalCounter()
            guard cache <= input else { throw ProviderFailure.format }
            let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
            let raw = String(decoding: try encoder.encode(json), as: UTF8.self)
            let eventID = json["event_id"].stringValue ?? json["eventId"].stringValue ?? context["event_id"].stringValue ?? RecordCoding.hash([raw])
            let identity = "\(id):unified:\(eventID)"
            guard seen.insert(identity).inserted else { return }
            let model = models[scope] ?? "Unknown"
            if let process {
                processSessions[process, default: []].insert(id)
                if model == "Unknown", let fallback = processModels[process] { pendingModels[identity] = (process, fallback) }
            }
            if sessions[id] == nil { sessions[id] = ProviderSession(id: id, title: "Grok · \(rawID.prefix(8))", path: url.path, client: "Grok CLI") }
            sessions[id]?.events.append(.init(id: identity, model: model, timestamp: date, input: input - cache, output: output, cacheRead: cache,
                origin: .init(group: id, priority: 2)))
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { sessions[$0] }.map { session in
            var session = session
            session.events = session.events.map { event in
                guard let candidate = pendingModels[event.id], processSessions[candidate.process]?.count == 1 else { return event }
                return ProviderEvent(id: event.id, model: candidate.model, timestamp: event.timestamp,
                    input: event.input, output: event.output, cacheRead: event.cacheRead, origin: event.origin)
            }
            return session
        })
    }
}
