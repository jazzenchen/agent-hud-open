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
        func add(_ key: String, _ title: String, _ used: Double, reset: Date?, duration: Double?) throws {
            guard used.isFinite, used >= 0 else { throw ProviderFailure.format }
            quota.windows.append(.init(id: credential.pool.windowID(key), label: title + " · " + credential.pool.label,
                remaining: max(0, 100 - used), reset: reset, duration: duration))
        }
        switch credential.service {
        case .kimi, .kimiGlobal:
            func window(_ detail: ProviderJSON, key: String, title: String, duration: Double?) throws {
                guard let limit = numeric(detail["limit"]), limit > 0,
                      let used = numeric(detail["used"]) ?? numeric(detail["remaining"]).map({ max(0, limit - $0) }) else { throw ProviderFailure.format }
                try add(key, title, used / limit * 100, reset: ProviderDate.iso(detail["resetTime"].stringValue), duration: duration)
            }
            if root["usage"].objectValue != nil { try window(root["usage"], key: "weekly", title: "7d", duration: 7 * 86400) }
            for entry in root["limits"].arrayValue ?? [] {
                let windowSpec = entry["window"]
                guard let count = numeric(windowSpec["duration"]), count > 0,
                      let unit = windowSpec["timeUnit"].stringValue,
                      let multiplier = ["TIME_UNIT_MINUTE": 60.0, "TIME_UNIT_HOUR": 3600, "TIME_UNIT_DAY": 86400, "TIME_UNIT_WEEK": 604800][unit] else { throw ProviderFailure.format }
                let duration = count * multiplier
                guard duration.isFinite, duration <= 253402300799 else { throw ProviderFailure.format }
                try window(entry["detail"], key: "limit:\(unit):\(count)", title: "\(Int(duration / 60))m", duration: duration)
            }
            quota.plan = root["membership"]["level"].stringValue
        case .go:
            guard root["usage"].objectValue != nil else { throw ProviderFailure.format }
            for key in ["rolling", "weekly", "monthly"] {
                let value = root["usage"][key]
                guard value != .null else { continue }
                guard let percent = numeric(value["percent"]) else { throw ProviderFailure.format }
                let reset = numeric(value["resetInSec"]).flatMap { $0 <= 253402300799 - now.timeIntervalSince1970 ? now.addingTimeInterval($0) : nil }
                    ?? ProviderDate.iso(value["resetTime"].stringValue)
                // Direct API percentage is 0...100: 0.5 means 0.5%, never 50%.
                try add(key, key, percent, reset: reset, duration: key == "rolling" ? 5 * 3600 : key == "weekly" ? 7 * 86400 : nil)
            }
        case .glmChina, .glmGlobal:
            guard root["success"].boolValue == true, root["code"].numberValue == 200,
                  let limits = root["data"]["limits"].arrayValue else { throw ProviderFailure.format }
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
                let label = type == "TIME_LIMIT" ? "MCP" : (duration.map { "\(Int($0 / 60))m" } ?? type)
                try add("\(type):\(unit):\(count)", label, percent, reset: reset, duration: type == "TIME_LIMIT" && unit == 5 && count == 1 ? nil : duration)
            }
            quota.plan = root["data"]["planName"].stringValue
        }
        guard !quota.windows.isEmpty, Set(quota.windows.map(\.id)).count == quota.windows.count else { throw ProviderFailure.format }
        return quota
    }
}
