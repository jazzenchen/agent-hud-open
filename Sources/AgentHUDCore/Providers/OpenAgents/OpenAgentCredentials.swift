import AgentHUDSupport
import Foundation
import CryptoKit

struct OpenAgentCredential: Sendable {
    enum Service: String, Sendable { case kimi, kimiGlobal, glmChina, glmGlobal, go }
    let service: Service
    let token: String
    let pool: BillingPool
    var headers: [String: String] = [:]
    var clients: Set<String> = []
    var endpoint: URL {
        switch service {
        case .kimi: URL(string: "https://api.kimi.com/coding/v1/usages")!
        case .kimiGlobal: URL(string: "https://api.kimi.ai/coding/v1/usages")!
        case .go: URL(string: "https://opencode.ai/zen/go/v1/usage")!
        case .glmChina: URL(string: "https://open.bigmodel.cn/api/monitor/usage/quota/limit" + (pool.organization == nil ? "" : "?type=2"))!
        case .glmGlobal: URL(string: "https://api.z.ai/api/monitor/usage/quota/limit" + (pool.organization == nil ? "" : "?type=2"))!
        }
    }
}

/// Read-only discovery. No shell commands, token refresh, browser cookies, or credential writes.
/// Different keys remain separate unless provider-owned account identity proves they share a pool.
enum OpenAgentCredentials {
    static func read(_ url: URL, json5: Bool = false) -> ProviderJSON {
        guard let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 1024 * 1024,
              let data = try? Data(contentsOf: url) else { return .null }
        let decoder = JSONDecoder(); decoder.allowsJSON5 = json5
        return (try? decoder.decode(ProviderJSON.self, from: data)) ?? .null
    }
    static func service(provider: String, baseURL: String? = nil, client: OpenAgentSource? = nil) -> OpenAgentCredential.Service? {
        if let baseURL {
            guard let url = URL(string: baseURL), url.scheme == "https", url.user == nil, url.password == nil, url.query == nil, url.fragment == nil, url.port == nil || url.port == 443 else { return nil }
            let path = url.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            switch url.host?.lowercased() {
            case "api.kimi.ai" where ["coding", "coding/v1"].contains(path): return .kimiGlobal
            case "api.kimi.com" where ["coding", "coding/v1"].contains(path): return .kimi
            case "api.z.ai" where ["api/coding/paas/v4", "api/anthropic"].contains(path): return .glmGlobal
            case "open.bigmodel.cn" where ["api/coding/paas/v4", "api/anthropic"].contains(path): return .glmChina
            case "opencode.ai" where path == "zen/go/v1": return .go
            default: return nil // API endpoints and proxies are separate billing products.
            }
        }
        switch provider {
        case "kimi-for-coding", "kimi-coding", "kimi-code": return .kimi
        case "zai" where client == .pi: return .glmGlobal
        case "zai-coding-cn" where client == .pi: return .glmChina
        case "zai-coding-plan": return .glmGlobal
        case "zhipuai-coding-plan": return .glmChina
        case "opencode-go": return .go
        default: return nil
        }
    }
    static func credential(_ service: OpenAgentCredential.Service, token: String, client: String,
                           accountID: String? = nil, organization: String? = nil, project: String? = nil,
                           headers: [String: String] = [:]) -> OpenAgentCredential {
        let isKimi = service == .kimi || service == .kimiGlobal
        let provider = isKimi ? "Kimi" : service == .go ? "OpenCode Go" : "GLM"
        let realm = service == .glmChina || service == .kimi ? "CN" : "International"
        let product = isKimi ? "kimi-code" : service == .go ? "opencode-go" : "glm-coding-plan"
        let pool = BillingPool(provider: provider, realm: realm, product: .plan,
            scope: RecordCoding.hash([accountID ?? token]), evidence: accountID == nil ? .credential : .account,
            organization: organization.map { RecordCoding.hash([$0]) }, project: project.map { RecordCoding.hash([$0]) }, entitlement: product)
        return .init(service: service, token: token, pool: pool, headers: headers, clients: [client])
    }
    static func discover(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                         environment env: [String: String] = ProcessInfo.processInfo.environment,
                         now: Date = Date()) -> [OpenAgentCredential] {
        let paths = OpenAgentPaths(home: home, environment: env)
        var found: [OpenAgentCredential] = []
        func add(_ service: OpenAgentCredential.Service?, _ key: String?, _ client: String) {
            guard let service, let key, !key.isEmpty else { return }
            found.append(credential(service, token: key, client: client))
        }
        add(env["KIMI_CODE_BASE_URL"].map { service(provider: "", baseURL: $0) } ?? .kimi, env["KIMI_CODE_API_KEY"], "Kimi")
        add(.go, env["OPENCODE_GO_API_KEY"], "OpenCode")
        add(.glmGlobal, env["ZAI_API_KEY"], "Pi")
        add(.glmChina, env["ZAI_CODING_CN_API_KEY"], "Pi")
        add(.kimi, env["KIMI_API_KEY"], "Pi")
        let region = env["Z_AI_REGION"] ?? "global"
        let scope = env["Z_AI_USAGE_SCOPE"] ?? "personal"
        if ["global", "bigmodel-cn"].contains(region), ["personal", "team"].contains(scope), let key = env["Z_AI_API_KEY"], !key.isEmpty {
            if scope == "personal" { add(region == "global" ? .glmGlobal : .glmChina, key, "GLM") }
            else if let org = env["Z_AI_ORGANIZATION"], !org.isEmpty, let project = env["Z_AI_PROJECT"], !project.isEmpty {
                found.append(credential(region == "global" ? .glmGlobal : .glmChina, token: key, client: "GLM",
                    organization: org, project: project, headers: ["Bigmodel-Organization": org, "Bigmodel-Project": project]))
            }
        }
        for name in ["BIGMODEL_API_KEY", "ZHIPU_API_KEY", "ZHIPUAI_API_KEY", "GLM_API_KEY"] { add(.glmChina, env[name], "GLM") }
        add(service(provider: "", baseURL: env["ANTHROPIC_BASE_URL"]), env["ANTHROPIC_AUTH_TOKEN"] ?? env["ANTHROPIC_API_KEY"], "Claude")
        let claude = read(home.appendingPathComponent(".claude/settings.json"))["env"]
        add(service(provider: "", baseURL: claude["ANTHROPIC_BASE_URL"].stringValue),
            claude["ANTHROPIC_AUTH_TOKEN"].stringValue ?? claude["ANTHROPIC_API_KEY"].stringValue, "Claude")

        // Explicit endpoint overrides take precedence over built-in provider names.
        let configRoot = URL(fileURLWithPath: env["XDG_CONFIG_HOME"] ?? home.appendingPathComponent(".config").path)
        let openJSON = configRoot.appendingPathComponent("opencode/opencode.json")
        let openJSONC = configRoot.appendingPathComponent("opencode/opencode.jsonc")
        let configs = [openJSON, openJSONC].filter { FileManager.default.fileExists(atPath: $0.path) }.map { read($0, json5: true) }
        let validConfig = configs.allSatisfy { $0.objectValue != nil }
        func openBase(_ provider: String) -> String? {
            configs.reversed().compactMap { $0["provider"][provider]["options"]["baseURL"].stringValue }.first
        }
        for (provider, auth) in read(paths.openCode.appendingPathComponent("auth.json")).objectValue ?? [:] where validConfig && auth["type"].stringValue == "api" {
            add(service(provider: provider, baseURL: openBase(provider), client: .opencode), auth["key"].stringValue, "OpenCode")
        }
        let piConfig = read(paths.pi.appendingPathComponent("models.json"))["providers"]
        for (provider, auth) in read(paths.pi.appendingPathComponent("auth.json")).objectValue ?? [:] {
            let resolvedService = service(provider: provider, baseURL: piConfig[provider]["baseUrl"].stringValue, client: .pi)
            if auth["type"].stringValue == "api_key" {
                // Pi supports executable key resolvers; never execute them from the HUD.
                guard let key = auth["key"].stringValue, !key.hasPrefix("!") else { continue }
                add(resolvedService, env[key] ?? key, "Pi")
            } else if provider == "kimi-coding", resolvedService == .kimi, auth["type"].stringValue == "oauth",
                      env["KIMI_CODE_OAUTH_HOST"] == nil, env["KIMI_OAUTH_HOST"] == nil,
                      let token = auth["access"].stringValue, !token.isEmpty,
                      let expires = auth["expires"].numberValue, expires > (now.timeIntervalSince1970 + 60) * 1000 {
                add(.kimi, token, "Pi")
            }
        }
        // Native slots are derived by the official OAuth toolkit. Region-specific credentials
        // are never sent to another realm or a custom endpoint; no marker/config guessing.
        let allowedBase = env["KIMI_CODE_BASE_URL"].flatMap { service(provider: "", baseURL: $0) }
        let customBase = env["KIMI_CODE_BASE_URL"] != nil && allowedBase != .kimi && allowedBase != .kimiGlobal
        let oauthHost = env["KIMI_CODE_OAUTH_HOST"] ?? env["KIMI_OAUTH_HOST"]
        let customOAuth = oauthHost != nil && oauthHost != "https://auth.kimi.com" && oauthHost != "https://auth.kimi.ai"
        if !customBase && !customOAuth {
            for service in [OpenAgentCredential.Service.kimi, .kimiGlobal] {
                if let allowedBase, allowedBase != service { continue }
                if let oauthHost, (oauthHost == "https://auth.kimi.ai") != (service == .kimiGlobal) { continue }
                let auth = read(paths.kimi.appendingPathComponent("credentials/" + kimiStorageName(service) + ".json"))
                guard let token = auth["access_token"].stringValue, !token.isEmpty,
                      let expiry = auth["expires_at"].numberValue, expiry > now.timeIntervalSince1970 + 60 else { continue }
                var headers: [String: String] = [:]
                if let device = try? String(contentsOf: paths.kimi.appendingPathComponent("device_id"), encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines), !device.isEmpty {
                    headers["X-Msh-Device-Id"] = device
                }
                found.append(credential(service, token: token, client: "Kimi", headers: headers))
            }
        }
        return merge(found)
    }
    static func kimiStorageName(_ service: OpenAgentCredential.Service) -> String {
        guard service == .kimiGlobal else { return "kimi-code" }
        let serialized = #"{"oauthHost":"https://auth.kimi.ai","baseUrl":"https://api.kimi.ai/coding/v1"}"#
        let hash = SHA256.hash(data: Data(serialized.utf8)).map { String(format: "%02x", $0) }.joined()
        return "kimi-code-env-" + hash.prefix(16)
    }

    static func merge(_ candidates: [OpenAgentCredential]) -> [OpenAgentCredential] {
        var values: [String: OpenAgentCredential] = [:]
        for candidate in candidates {
            if var existing = values[candidate.pool.id] { existing.clients.formUnion(candidate.clients); values[candidate.pool.id] = existing }
            else { values[candidate.pool.id] = candidate }
        }
        return values.values.sorted { $0.pool.id < $1.pool.id }
    }
}
