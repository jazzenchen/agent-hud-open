import AgentHUDSupport
import Foundation

// Protocol/SQLite auth derived from CodexBar; event session identity follows Tokscale. See THIRD_PARTY_NOTICES.txt.
actor CursorClient {
    struct Session: Sendable { let account: String; let cookie: String; var email: String? = nil; var token = "" }
    let database: URL
    /// The agent CLI's folder, `~/.cursor`, where its chats and ACP sessions keep their names.
    let agentFolder: URL
    let http: ProviderHTTP
    private var cached: (at: Date, account: String, since: Date, result: ProviderSessions)?
    private var fetches = 0

    var savedSessions: ProviderSessions {
        var result = cached?.result ?? ProviderSessions()
        result.revision = fetches
        // The dashboard is asked from local midnight.
        result.start = cached?.since
        return result
    }

    init(database: URL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Cursor/User/globalStorage/state.vscdb"),
         agentFolder: URL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cursor"),
         http: ProviderHTTP = ProviderHTTP()) {
        self.database = database; self.agentFolder = agentFolder; self.http = http
    }

    func session(now: Date = Date()) throws -> Session {
        let reader = try ReadOnlySQLite(database)
        try reader.requireTable("ItemTable")
        var token: String?, email: String?
        try reader.rows("SELECT value FROM ItemTable WHERE key = ?", strings: ["cursorAuth/accessToken"]) { row in
            token = ReadOnlySQLite.text(row, 0)
        }
        try reader.rows("SELECT value FROM ItemTable WHERE key = ?", strings: ["cursorAuth/cachedEmail"]) { row in
            email = ReadOnlySQLite.text(row, 0)
        }
        guard let token else { throw ProviderFailure.login("Cursor") }
        var session = try Self.session(token: token, now: now)
        session.email = email
        return session
    }

    static func session(token: String, now: Date) throws -> Session {
        let parts = token.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3, !token.contains("\0"), !token.contains("\r"), !token.contains("\n") else { throw ProviderFailure.login("Cursor") }
        var base64 = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        base64 += String(repeating: "=", count: (4 - base64.count % 4) % 4)
        guard let data = Data(base64Encoded: base64), let json = try? ProviderJSON.read(data),
              let exp = json["exp"].numberValue, exp > now.timeIntervalSince1970 + 60,
              let sub = json["sub"].stringValue, let user = sub.split(separator: "|").last.map(String.init), !user.isEmpty,
              user.unicodeScalars.allSatisfy(CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-")).contains) else {
            throw ProviderFailure.login("Cursor")
        }
        return Session(account: RecordCoding.hash([user]), cookie: "WorkosCursorSessionToken=\(user)%3A%3A\(token)", token: token)
    }

    func quota() async throws -> ProviderQuota {
        guard FileManager.default.fileExists(atPath: database.path) else { return ProviderQuota() }
        let auth = try session()
        let json = try await http.json(URL(string: "https://cursor.com/api/usage-summary")!, headers: ["Cookie": auth.cookie])
        var quota = try Self.parseQuota(json)
        // The session's user id is confirmed by the dashboard accepting it.
        quota.account = ProviderAccount(provider: "Cursor", user: auth.account, workspace: "", evidence: .account)
        quota.label = auth.email
        return quota
    }

    static func parseQuota(_ json: ProviderJSON) throws -> ProviderQuota {
        guard json["individualUsage"].objectValue != nil || json["teamUsage"].objectValue != nil || json["membershipType"].stringValue != nil else {
            throw ProviderFailure.format
        }
        let end = DateParsing.internet(json["billingCycleEnd"].stringValue)
        let start = DateParsing.internet(json["billingCycleStart"].stringValue)
        let duration = ProviderDate.period(start: start, end: end)
        let plan = json["individualUsage"]["plan"]
        func ratio(_ value: ProviderJSON) -> Double? {
            guard value["enabled"].boolValue != false, let used = value["used"].numberValue, used >= 0,
                  let limit = value["limit"].numberValue, limit > 0 else { return nil }
            return used / limit * 100
        }
        var result = ProviderQuota(plan: json["membershipType"].stringValue)
        let main = plan["enabled"].boolValue == false ? nil : plan["totalPercentUsed"].numberValue ?? ratio(plan)
        // Cursor's own names for its usage pools and limits. Its first pool is its own models, which Cursor calls
        // first-party, so the short name says so, where "Cursor Models" would read as the whole vendor; the other pool,
        // third-party models, follows. The two pools each limit some of the plan's models; the rest are plan-wide.
        let rows: [(String, String, String, Double?)] = [
            ("cursor", L10n.text("包含用量", "Included usage"), L10n.text("包含用量", "Included"), main),
            ("cursor:models", L10n.text("Cursor 模型", "Cursor Models"), L10n.text("第一方模型", "First-party"),
             plan["autoPercentUsed"].numberValue),
            ("cursor:third-party", L10n.text("其他模型", "Other Models"), L10n.text("第三方模型", "Third-party"),
             plan["apiPercentUsed"].numberValue),
            ("cursor:personal", L10n.text("个人支出限额", "Individual spending limit"), L10n.text("支出限额", "Spend limit"),
             ratio(json["individualUsage"]["overall"])),
            ("cursor:team", L10n.text("共享用量", "Pooled usage"), L10n.text("共享用量", "Pooled"), ratio(json["teamUsage"]["pooled"])),
            ("cursor:extra", L10n.text("按需用量", "On-demand usage"), L10n.text("按需用量", "On-demand"),
             ratio(json["individualUsage"]["onDemand"]))
        ]
        for (id, label, short, used) in rows {
            guard let used, used.isFinite, used >= 0 else { continue }
            result.windows.append(.init(id: id, label: label, remaining: QuotaMath.remaining(usedPercent: used), reset: end, duration: duration,
                                        shortLabel: short, allModels: id != "cursor:models" && id != "cursor:third-party"))
        }
        if result.windows.isEmpty {
            result.displayNotice = L10n.text("Cursor 已连接，当前计划未提供额度比例", "Cursor is connected; this plan reports no quota percentage")
        }
        return result
    }

    func sessions(since: Date) async -> ProviderSessions {
        guard FileManager.default.fileExists(atPath: database.path) else { return ProviderSessions() }
        let now = Date(), start = Calendar.current.startOfDay(for: since)
        let auth: Session
        do { auth = try session(now: now) }
        catch {
            cached = nil
            fetches += 1
            return ProviderSessions(notice: error.localizedDescription)
        }
        if cached?.account != auth.account { cached = nil }
        do {
            let events = try await fetchEvents(auth: auth, since: start, until: now)
            let result = named(try Self.parseEvents(events, account: auth.account), account: auth.account)
            cached = (now, auth.account, start, result)
            fetches += 1
            return result
        } catch {
            // A failed read keeps the sessions the last one found, with a notice, until the next account step.
            let message = L10n.text("Cursor 账户用量读取失败，稍后自动重试", "Cursor account usage could not be read; retrying shortly")
            var previous = cached?.result ?? ProviderSessions()
            previous.notice = message
            cached = (now, auth.account, start, previous)
            return previous
        }
    }

    /// The routines of the account's Grok Bot agents as the Bot service records them, each with the runs it lists. Grok
    /// Bot signs in with the Cursor account, so the service answers Cursor's own token.
    func grokBotAutomations(agentIDs: [String]) async throws -> ProviderJSON {
        let auth = try session()
        return try await http.json(URL(string: "https://api2.cursor.sh/aiserver.v1.GrokBotService/ListGrokBotAccountAutomations")!,
            headers: ["Authorization": "Bearer \(auth.token)", "Connect-Protocol-Version": "1"],
            body: .object(["agentIds": .array(agentIDs.map(ProviderJSON.string))]))
    }

    private func fetchEvents(auth: Session, since: Date, until: Date) async throws -> [ProviderJSON] {
        var pages: [[ProviderJSON]] = [], expected: Int?, complete = false
        let deadline = Date().addingTimeInterval(25)
        for page in 1...200 {
            try Task.checkCancellation()
            guard Date() < deadline else { throw ProviderFailure.limit }
            let json = try await http.json(URL(string: "https://cursor.com/api/dashboard/get-filtered-usage-events")!,
                headers: ["Cookie": auth.cookie, "Origin": "https://cursor.com"], body: .object([
                    "page": .integer(Int64(page)), "pageSize": .integer(1000),
                    "startDate": .string(String(RecordCoding.milliseconds(since))), "endDate": .string(String(RecordCoding.milliseconds(until)))
                ]))
            let count = json["totalUsageEventsCount"].countValue ?? json["totalUsageEventsCount"].stringValue.flatMap(Int.init)
            if let count {
                guard count >= 0, expected == nil || expected == count else { throw ProviderFailure.format }
                expected = count
            }
            let events: [ProviderJSON]
            if let array = json["usageEventsDisplay"].arrayValue { events = array }
            else if json.objectValue?.isEmpty == true {
                guard expected == nil || expected == 0 else { throw ProviderFailure.format }
                expected = 0; events = []
            } else if Set(json.objectValue?.keys.map { $0 } ?? []) == ["totalUsageEventsCount"], count != nil { events = [] }
            else { throw ProviderFailure.format }
            pages.append(events)
            if events.count < 1000 { complete = true; break }
        }
        guard complete else { throw ProviderFailure.limit }
        return try Self.reconcile(pages: pages, expected: expected)
    }

    static func reconcile(pages: [[ProviderJSON]], expected: Int?) throws -> [ProviderJSON] {
        let total = pages.reduce(0) { $0 + $1.count }
        guard let expected else { return pages.flatMap { $0 } }
        guard total >= expected else { throw ProviderFailure.format }
        var remove = total - expected, result = pages.first ?? []
        for index in pages.indices.dropFirst() {
            let previous = pages[index - 1], current = pages[index]
            var overlap = 0
            if remove > 0 {
                for n in stride(from: min(previous.count, current.count, remove), through: 1, by: -1) {
                    if Array(previous.suffix(n)) == Array(current.prefix(n)) { overlap = n; break }
                }
            }
            result += current.dropFirst(overlap); remove -= overlap
        }
        guard remove == 0, result.count == expected else { throw ProviderFailure.format }
        return result
    }

    /// The names Cursor keeps on this Mac for the conversations the events name, read when the events are: an IDE
    /// composer's in its header, an agent CLI chat's or ACP session's in its `meta.json`. A conversation that ran on
    /// another machine or in the cloud keeps its placeholder. A composer whose header says it was archived is too.
    func named(_ result: ProviderSessions, account: String) -> ProviderSessions {
        let prefix = "cursor-account:\(account):"
        let conversations = result.sessions.map { String($0.id.dropFirst(prefix.count)) }.filter { !$0.hasPrefix("unassigned-") }
        guard !conversations.isEmpty else { return result }
        var titles: [String: String] = [:], archived = Set<String>()
        if let reader = try? ReadOnlySQLite(database), let list = try? JSONSerialization.data(withJSONObject: conversations) {
            // Only the name and the archive flag leave the database; a header's other fields and the composers' contents
            // are never read.
            try? reader.rows("SELECT composerId, json_extract(value, '$.name'), json_extract(value, '$.isArchived') FROM composerHeaders WHERE composerId IN (SELECT value FROM json_each(?))",
                             strings: [String(decoding: list, as: UTF8.self)]) { row in
                guard let id = ReadOnlySQLite.text(row, 0) else { return }
                if let title = SessionTitle.named(ReadOnlySQLite.text(row, 1)) { titles[id] = title }
                if ReadOnlySQLite.text(row, 2) == "1" { archived.insert(id) }
            }
        }
        let chats = (try? FileManager.default.contentsOfDirectory(at: agentFolder.appendingPathComponent("chats"), includingPropertiesForKeys: nil)) ?? []
        for id in conversations where titles[id] == nil {
            for folder in [agentFolder.appendingPathComponent("acp-sessions")] + chats {
                let meta = folder.appendingPathComponent(id).appendingPathComponent("meta.json")
                guard FileManager.default.fileExists(atPath: meta.path) else { continue }
                // The agent calls a chat "New Agent" until its title arrives.
                if let title = SessionTitle.named((try? ProviderFiles.json(meta))?["title"].stringValue), title != "New Agent" { titles[id] = title }
                break
            }
        }
        var result = result
        result.sessions = result.sessions.map {
            var item = $0
            let id = String(item.id.dropFirst(prefix.count))
            if let title = titles[id] { item.title = title }
            item.archived = archived.contains(id)
            return item
        }
        return result
    }

    static func parseEvents(_ rows: [ProviderJSON], account: String) throws -> ProviderSessions {
        var sessions: [String: ProviderSession] = [:], occurrences: [String: Int] = [:]
        for row in rows {
            guard row["tokenUsage"].objectValue != nil else { continue } // Metered-only events do not establish token counts.
            let usage = row["tokenUsage"]
            guard let date = ProviderDate.milliseconds(row["timestamp"]), let model = row["model"].stringValue, !model.isEmpty,
                  let input = usage["inputTokens"].countValue, let output = usage["outputTokens"].countValue else { throw ProviderFailure.format }
            let cache = try usage["cacheReadTokens"].optionalCounter(), write = try usage["cacheWriteTokens"].optionalCounter()
            guard input <= Int.max - write else { throw ProviderFailure.format }
            let conversation = row["conversationId"].stringValue
            let identity = RecordCoding.hash([account, conversation ?? "", String(RecordCoding.milliseconds(date)), model,
                String(input), String(output), String(cache), String(write)])
            let ordinal = occurrences[identity, default: 0]; occurrences[identity] = ordinal + 1
            let event = ProviderEvent(id: "\(identity):\(ordinal)", model: model, timestamp: date, input: input + write, output: output, cacheRead: cache,
                                      cacheWrite: write)
            // ID-less events remain unassigned; do not invent a multi-request conversation from their timestamps.
            let id = "cursor-account:\(account):\(conversation ?? "unassigned-" + identity + "-" + String(ordinal))"
            if sessions[id] == nil {
                let subagent = conversation.flatMap(GrokBotUsage.subagentID)
                let title = subagent.map { L10n.text("Grok Bot 子代理", "Grok Bot subagent") + " · " + $0.prefix(8) }
                    ?? conversation.map { "Cursor · \($0.prefix(8))" }
                    ?? L10n.text("Cursor 未归属请求", "Cursor unassigned request")
                sessions[id] = ProviderSession(id: id, title: title, client: subagent == nil ? "Cursor" : "Grok Bot", accountWide: true)
            }
            sessions[id]?.events.append(event)
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { sessions[$0] })
    }
}
