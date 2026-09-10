import AgentHUDSupport
import Foundation

// Protocol/SQLite auth derived from CodexBar; event session identity follows Tokscale. See THIRD_PARTY_NOTICES.txt.
actor CursorClient {
    struct Session: Sendable { let account: String; let cookie: String }
    let database: URL
    let http: ProviderHTTP
    private var cached: (at: Date, account: String, since: Date, result: ProviderSessions)?

    init(database: URL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Cursor/User/globalStorage/state.vscdb"),
         http: ProviderHTTP = ProviderHTTP()) {
        self.database = database; self.http = http
    }

    func session(now: Date = Date()) throws -> Session {
        let reader = try ReadOnlySQLite(database)
        try reader.requireTable("ItemTable")
        var token: String?
        try reader.rows("SELECT value FROM ItemTable WHERE key = ?", strings: ["cursorAuth/accessToken"]) { row in
            token = ReadOnlySQLite.text(row, 0)
        }
        guard let token else { throw ProviderFailure.login("Cursor") }
        return try Self.session(token: token, now: now)
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
        return Session(account: RecordCoding.hash([user]), cookie: "WorkosCursorSessionToken=\(user)%3A%3A\(token)")
    }

    func quota() async throws -> ProviderQuota {
        guard FileManager.default.fileExists(atPath: database.path) else { return ProviderQuota() }
        let auth = try session()
        let json = try await http.json(URL(string: "https://cursor.com/api/usage-summary")!, headers: ["Cookie": auth.cookie])
        return try Self.parseQuota(json)
    }

    static func parseQuota(_ json: ProviderJSON) throws -> ProviderQuota {
        guard json["individualUsage"].objectValue != nil || json["teamUsage"].objectValue != nil || json["membershipType"].stringValue != nil else {
            throw ProviderFailure.format
        }
        let end = ProviderDate.iso(json["billingCycleEnd"].stringValue)
        let start = ProviderDate.iso(json["billingCycleStart"].stringValue)
        let duration = ProviderDate.period(start: start, end: end)
        let plan = json["individualUsage"]["plan"]
        func ratio(_ value: ProviderJSON) -> Double? {
            guard value["enabled"].boolValue != false, let used = value["used"].numberValue, used >= 0,
                  let limit = value["limit"].numberValue, limit > 0 else { return nil }
            return used / limit * 100
        }
        var result = ProviderQuota(plan: json["membershipType"].stringValue)
        let main = plan["enabled"].boolValue == false ? nil : plan["totalPercentUsed"].numberValue ?? ratio(plan)
        let rows: [(String, String, Double?)] = [
            ("cursor", L10n.text("套餐总额度", "Plan usage"), main),
            ("cursor:models", L10n.text("Cursor 模型", "Cursor models"), plan["autoPercentUsed"].numberValue),
            ("cursor:third-party", L10n.text("第三方模型", "Third-party models"), plan["apiPercentUsed"].numberValue),
            ("cursor:personal", L10n.text("个人预算", "Personal budget"), ratio(json["individualUsage"]["overall"])),
            ("cursor:team", L10n.text("团队共享额度", "Team pool"), ratio(json["teamUsage"]["pooled"])),
            ("cursor:extra", L10n.text("额外用量预算", "Extra usage budget"), ratio(json["individualUsage"]["onDemand"]))
        ]
        for (id, label, used) in rows {
            guard let used, used.isFinite, used >= 0 else { continue }
            result.windows.append(.init(id: id, label: label, remaining: max(0, 100 - used), reset: end, duration: duration))
        }
        if result.windows.isEmpty {
            result.notice = L10n.text("Cursor 已连接，当前计划未提供额度比例", "Cursor is connected; this plan reports no quota percentage")
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
            return ProviderSessions(notice: error.localizedDescription)
        }
        if let cached, cached.account == auth.account, cached.since == start, now.timeIntervalSince(cached.at) < 300 { return cached.result }
        if cached?.account != auth.account { cached = nil }
        do {
            let events = try await fetchEvents(auth: auth, since: start, until: now)
            let result = try Self.parseEvents(events, account: auth.account)
            cached = (now, auth.account, start, result)
            return result
        } catch {
            // Cache retry failures too: polling local sessions every five seconds must not hammer the dashboard.
            let message = L10n.text("Cursor 账户用量读取失败，稍后自动重试", "Cursor account usage could not be read; retrying shortly")
            var previous = cached?.result ?? ProviderSessions()
            previous.notice = message
            cached = (now, auth.account, start, previous)
            return previous
        }
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
            let event = ProviderEvent(id: "\(identity):\(ordinal)", model: model, timestamp: date, input: input + write, output: output, cacheRead: cache)
            // ID-less events remain unassigned; do not invent a multi-request conversation from their timestamps.
            let id = "cursor-account:\(account):\(conversation ?? "unassigned-" + identity + "-" + String(ordinal))"
            if sessions[id] == nil {
                sessions[id] = ProviderSession(id: id, title: conversation.map { "Cursor · \($0.prefix(8))" }
                    ?? L10n.text("Cursor 未归属请求", "Cursor unassigned request"), client: "Cursor", accountWide: true)
            }
            sessions[id]?.events.append(event)
        }
        return ProviderSessions(sessions: sessions.keys.sorted().compactMap { sessions[$0] })
    }
}
