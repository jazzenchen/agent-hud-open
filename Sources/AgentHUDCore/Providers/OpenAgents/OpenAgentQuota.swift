import AgentHUDSupport
import Foundation

/// Direct, documented metadata endpoints only. No inference/model requests are issued.
struct OpenAgentQuotaClient: Sendable {
    var http = ProviderHTTP()
    func fetch(_ credential: OpenAgentCredential, now: Date) async throws -> ProviderQuota {
        var headers = credential.headers
        headers["Authorization"] = "Bearer \(credential.token)"
        let json = try await http.json(credential.endpoint, headers: headers)
        return try Self.parse(json, credential: credential, now: now)
    }
    /// The official profile proves account identity across API keys and CLI OAuth tokens.
    /// Identity failure leaves credential-scoped quota available, without guessing a shared account.
    func identify(_ credential: OpenAgentCredential) async throws -> OpenAgentCredential {
        guard credential.service == .kimi || credential.service == .kimiGlobal else { return credential }
        var headers = credential.headers
        headers["Authorization"] = "Bearer \(credential.token)"
        let url = credential.endpoint.deletingLastPathComponent().appendingPathComponent("me")
        let profile = try await http.json(url, headers: headers, timeout: 8)
        // Live /me responses can omit domain; the official profile parser defaults it to 0.
        let domain = profile["domain"] == .null ? 0
            : profile["domain"].countValue ?? profile["domain"].stringValue.flatMap(Int.init)
        guard let account = profile["user_id"].stringValue, !account.isEmpty,
              let domain, domain >= 0,
              let region = profile["region"].stringValue, !region.isEmpty else { throw ProviderFailure.format }
        let pool = credential.pool
        return .init(service: credential.service, token: credential.token,
            pool: BillingPool(provider: pool.provider, realm: pool.realm, product: pool.product,
                scope: RecordCoding.hash([account, String(domain), region]), evidence: .account,
                organization: pool.organization, project: pool.project, entitlement: pool.entitlement),
            headers: credential.headers, clients: credential.clients, expiresAt: credential.expiresAt)
    }

    static func numeric(_ value: ProviderJSON) -> Double? {
        let n = value.numberValue ?? value.stringValue.flatMap(Double.init)
        return n.flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
    }
    static func parse(_ root: ProviderJSON, credential: OpenAgentCredential, now: Date) throws -> ProviderQuota {
        var quota = ProviderQuota()
        /// A window's full name is its period in the vendor's own words and then its plan; the pool it belongs to is the
        /// account's label, not the window's. Its short name is its period.
        func add(_ key: String, _ name: (full: String, short: String), _ used: Double, reset: Date?, duration: Double?,
                 allModels: Bool = true) throws {
            guard used.isFinite, used >= 0 else { throw ProviderFailure.format }
            let plan = quota.plan.flatMap { $0.isEmpty ? nil : " · " + $0 } ?? ""
            quota.windows.append(.init(id: credential.pool.windowID(key), label: name.full + plan,
                remaining: QuotaMath.remaining(usedPercent: used), reset: reset, duration: duration, shortLabel: name.short,
                allModels: allModels))
        }
        switch credential.service {
        case .kimi, .kimiGlobal:
            quota.plan = root["membership"]["level"].stringValue
            // The current report, as Kimi's own CLI reads it: the share used and the reset of the 5 hours, of the week on
            // older plans and of the month's total, which holds the month's code share. Where it gives a window it is read
            // alone, the 5 hours and the week under the ids the earlier report gave them.
            let usages = root["usages"]
            let current: [(field: String, key: String, name: (full: String, short: String), duration: Double?)] = [
                ("limit_5h", "limit:TIME_UNIT_MINUTE:300.0", kimiName(.fiveHours), 18000), ("limit_7d", "weekly", kimiName(.week), 604800),
                ("limit_month_total", "monthly", (L10n.text("月总额度", "Monthly total quota"), WindowNames.Period.month.shortName), nil),
            ]
            for window in current {
                guard let ratio = numeric(usages[window.field]["used_ratio"]) else { continue }
                try add(window.key, window.name, ratio * 100, reset: DateParsing.internet(usages[window.field]["reset_time"].stringValue),
                        duration: window.duration)
            }
            guard quota.windows.isEmpty else { break }
            func window(_ detail: ProviderJSON, key: String, duration: Double) throws {
                guard let limit = numeric(detail["limit"]), limit > 0,
                      let used = numeric(detail["used"]) ?? numeric(detail["remaining"]).map({ max(0, limit - $0) }) else { throw ProviderFailure.format }
                try add(key, kimiName(WindowNames.Period(seconds: duration)), used / limit * 100,
                        reset: DateParsing.internet(detail["resetTime"].stringValue), duration: duration)
            }
            if root["usage"].objectValue != nil { try window(root["usage"], key: "weekly", duration: 7 * 86400) }
            for entry in root["limits"].arrayValue ?? [] {
                let windowSpec = entry["window"]
                guard let count = numeric(windowSpec["duration"]), count > 0,
                      let unit = windowSpec["timeUnit"].stringValue,
                      let multiplier = ["TIME_UNIT_MINUTE": 60.0, "TIME_UNIT_HOUR": 3600, "TIME_UNIT_DAY": 86400, "TIME_UNIT_WEEK": 604800][unit] else { throw ProviderFailure.format }
                let duration = count * multiplier
                guard duration.isFinite, duration <= 253402300799 else { throw ProviderFailure.format }
                try window(entry["detail"], key: "limit:\(unit):\(count)", duration: duration)
            }
        case .go:
            guard root["usage"].objectValue != nil else { throw ProviderFailure.format }
            // OpenCode's names for Go's limits; the service calls the 5 hours rolling.
            let names: [String: (full: String, short: String)] = [
                "rolling": (L10n.text("5 小时限制", "5-hour limit"), WindowNames.Period.fiveHours.shortName),
                "weekly": (L10n.text("每周限制", "Weekly limit"), WindowNames.Period.week.shortName),
                "monthly": (L10n.text("每月限制", "Monthly limit"), WindowNames.Period.month.shortName),
            ]
            for key in ["rolling", "weekly", "monthly"] {
                let value = root["usage"][key]
                guard value != .null else { continue }
                guard let percent = numeric(value["percent"]) else { throw ProviderFailure.format }
                let reset = DateParsing.internet(value["resetsAt"].stringValue)
                // Direct API percentage is 0...100: 0.5 means 0.5%, never 50%.
                try add(key, names[key]!, percent, reset: reset, duration: key == "rolling" ? 5 * 3600 : key == "weekly" ? 7 * 86400 : nil)
            }
        case .glmChina, .glmGlobal:
            guard root["success"].boolValue == true, root["code"].numberValue == 200,
                  let limits = root["data"]["limits"].arrayValue else { throw ProviderFailure.format }
            quota.plan = root["data"]["planName"].stringValue
            var windows: [(key: String, type: String, name: (full: String, short: String), used: Double, reset: Date?, duration: Double?)] = []
            for raw in limits {
                guard let type = raw["type"].stringValue, ["TOKENS_LIMIT", "CREDIT_LIMIT", "TIME_LIMIT"].contains(type) else { continue }
                guard let unit = raw["unit"].countValue, let count = raw["number"].countValue,
                      let percentage = numeric(raw["percentage"]) else { throw ProviderFailure.format }
                var percent = percentage
                if let limit = numeric(raw["usage"]), limit > 0 {
                    let current = numeric(raw["currentValue"])
                    let remaining = numeric(raw["remaining"])
                    if let used = remaining.map({ max(limit - $0, current ?? 0) }) ?? current { percent = max(0, used) / limit * 100 }
                }
                let duration = [1: 86400.0, 3: 3600, 5: 60, 6: 604800][unit].map { $0 * Double(count) }
                if let duration, !duration.isFinite || duration > 253402300799 { throw ProviderFailure.format }
                var reset = ProviderDate.milliseconds(raw["nextResetTime"])
                if type != "TIME_LIMIT", duration == 18000, let date = reset, date > now.addingTimeInterval(18060) { reset = nil }
                // The MCP window of older plans counts by the month.
                let monthly = type == "TIME_LIMIT" && unit == 5 && count == 1
                windows.append(("\(type):\(unit):\(count)", type, glmName(type, period: WindowNames.Period(seconds: duration), monthly: monthly),
                                percent, reset, monthly ? nil : duration))
            }
            // Token limits and credits of the same length, which an account does not normally have both of, are told
            // apart by their unit.
            let shorts = windows.map(\.name.short)
            for window in windows {
                var name = window.name
                if shorts.filter({ $0 == name.short }).count > 1, window.type != "TIME_LIMIT",
                   let period = WindowNames.Period(seconds: window.duration) {
                    name.short = (window.type == "CREDIT_LIMIT" ? "Credits " : "Tokens ") + period.afterWord
                }
                // The MCP window limits tool calls, not the plan's models.
                try add(window.key, name, window.used, reset: window.reset, duration: window.duration, allModels: window.type != "TIME_LIMIT")
            }
        }
        guard !quota.windows.isEmpty, Set(quota.windows.map(\.id)).count == quota.windows.count else { throw ProviderFailure.format }
        // Windows whose short names would still read alike keep their full names.
        quota.windows = zip(quota.windows, WindowNames.distinct(quota.windows.map(\.shortLabel))).map { window, short in
            var window = window
            window.shortLabel = short
            return window
        }
        return quota
    }

    /// Kimi's names for its limits: the 5-hour quota and the weekly quota of Kimi Code's plans, and any other length in
    /// the same words.
    static func kimiName(_ period: WindowNames.Period?) -> (full: String, short: String) {
        let full: String
        switch period {
        case .fiveHours: full = L10n.text("5 小时额度", "5-hour quota")
        case .week: full = L10n.text("周额度", "Weekly quota")
        case .day: full = L10n.text("每日额度", "Daily quota")
        case .month: full = L10n.text("月额度", "Monthly quota")
        case .year: full = L10n.text("年额度", "Annual quota")
        case .days(let count): full = L10n.text("\(count) 天额度", "\(count)-day quota")
        case .hours(let count): full = L10n.text("\(count) 小时额度", "\(count)-hour quota")
        case .minutes(let count): full = L10n.text("\(count) 分钟额度", "\(count)-minute quota")
        case nil: full = L10n.text("额度", "Quota")
        }
        return (full, period?.shortName ?? L10n.text("额度", "Quota"))
    }

    /// Zhipu's names for a GLM plan's limits: credits over 5 hours and a week on plans that meter credits, the 5-hour and
    /// weekly limits of older plans, and their monthly MCP usage. A limit of a unit Zhipu does not name keeps its type.
    static func glmName(_ type: String, period: WindowNames.Period?, monthly: Bool) -> (full: String, short: String) {
        if type == "TIME_LIMIT" { return (monthly ? L10n.text("MCP 每月用量", "MCP usage (1 month)") : L10n.text("MCP 用量", "MCP usage"), "MCP") }
        let credits = type == "CREDIT_LIMIT"
        guard let period else { return (type, credits ? "Credits" : "Tokens") }
        let full: String
        switch period {
        case .fiveHours: full = credits ? L10n.text("5 小时积分", "5-hour credits") : L10n.text("每 5 小时限额", "5-hour limit")
        case .week: full = credits ? L10n.text("每周积分", "Weekly credits") : L10n.text("每周限额", "Weekly limit")
        case .day: full = credits ? L10n.text("每日积分", "Daily credits") : L10n.text("每日限额", "Daily limit")
        case .month: full = credits ? L10n.text("每月积分", "Monthly credits") : L10n.text("每月限额", "Monthly limit")
        case .year: full = credits ? L10n.text("每年积分", "Annual credits") : L10n.text("每年限额", "Annual limit")
        case .days(let count):
            full = credits ? L10n.text("\(count) 天积分", "\(count)-day credits") : L10n.text("每 \(count) 天限额", "\(count)-day limit")
        case .hours(let count):
            full = credits ? L10n.text("\(count) 小时积分", "\(count)-hour credits") : L10n.text("每 \(count) 小时限额", "\(count)-hour limit")
        case .minutes(let count):
            full = credits ? L10n.text("\(count) 分钟积分", "\(count)-minute credits") : L10n.text("每 \(count) 分钟限额", "\(count)-minute limit")
        }
        return (full, period.shortName)
    }
}
