import Foundation

// Protocol reference: CodexBar GrokCreditsProxyFetcher/GrokAuth (MIT), pinned in THIRD_PARTY_NOTICES.md.
struct GrokClient: Sendable {
    var home = AdditionalLocalStore.grokHome
    var http = ProviderHTTP()

    func fetch() async throws -> ProviderQuota {
        guard FileManager.default.fileExists(atPath: home.path) else { return ProviderQuota() }
        let url = home.appendingPathComponent("auth.json")
        guard let json = try? ProviderFiles.json(url) else { throw ProviderFailure.login("Grok CLI") }
        let entry = try Self.credential(json, now: Date())
        guard let token = entry["key"].stringValue else { throw ProviderFailure.login("Grok CLI") }
        let headers = ["Authorization": "Bearer \(token)", "x-xai-token-auth": "xai-grok-cli"]
        let response = try await http.json(URL(string: "https://cli-chat-proxy.grok.com/v1/billing?format=credits")!, headers: headers)
        var quota = try Self.parse(response)
        if let settings = try? await http.json(URL(string: "https://cli-chat-proxy.grok.com/v1/settings")!, headers: headers, timeout: 2) {
            quota.plan = settings["subscription_tier_display"].stringValue ?? quota.plan
        }
        return quota
    }

    static func credential(_ json: ProviderJSON, now: Date) throws -> ProviderJSON {
        guard let entries = json.objectValue else { throw ProviderFailure.login("Grok CLI") }
        let valid = entries.filter { key, value in
            (key.hasPrefix("https://auth.x.ai::") || key == "https://accounts.x.ai/sign-in")
                && value["key"].stringValue?.isEmpty == false
                && ProviderDate.iso(value["expires_at"].stringValue).map { $0 > now } == true
        }.sorted { lhs, rhs in
            let a = lhs.key.hasPrefix("https://auth.x.ai::"), b = rhs.key.hasPrefix("https://auth.x.ai::")
            return a != b ? a : lhs.key < rhs.key
        }
        guard let selected = valid.first?.value else { throw ProviderFailure.login("Grok CLI") }
        if selected["principal_type"].stringValue?.lowercased() == "team" {
            throw UsageProviderError(L10n.text("Grok 团队账户尚未提供可读取的额度", "Grok team quota is not available through this interface"))
        }
        return selected
    }

    static func parse(_ response: ProviderJSON) throws -> ProviderQuota {
        let config = response["config"]
        guard config.objectValue != nil else { throw ProviderFailure.format }
        let period = config["currentPeriod"]
        let start = ProviderDate.iso(period["start"].stringValue)
        let end = ProviderDate.iso(period["end"].stringValue) ?? ProviderDate.iso(config["billingPeriodEnd"].stringValue)
        let duration = ProviderDate.period(start: start, end: end)
        var quota = ProviderQuota()
        if let used = config["creditUsagePercent"].numberValue, used >= 0 {
            let label: String
            switch period["type"].stringValue {
            case "USAGE_PERIOD_TYPE_WEEKLY": label = L10n.text("每周额度", "Weekly credits")
            case "USAGE_PERIOD_TYPE_MONTHLY": label = L10n.text("每月额度", "Monthly credits")
            default: label = L10n.text("订阅额度", "Subscription credits")
            }
            quota.windows.append(.init(id: "grok", label: label, remaining: max(0, 100 - used), reset: end, duration: duration))
        } else {
            quota.notice = L10n.text("Grok 已连接，但服务未返回已用额度", "Grok is connected, but used credits were not reported")
        }
        // Extra spending is a distinct budget, never a substitute for subscription consumption.
        if let cap = config["onDemandCap"]["val"].numberValue, cap > 0,
           let used = config["onDemandUsed"]["val"].numberValue, used >= 0 {
            quota.windows.append(.init(id: "grok:extra", label: L10n.text("额外用量预算", "Extra usage budget"),
                remaining: max(0, 100 - used / cap * 100), reset: end, duration: duration))
        }
        return quota
    }
}
