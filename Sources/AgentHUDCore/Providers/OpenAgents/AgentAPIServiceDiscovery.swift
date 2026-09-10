import Foundation

/// Reads explicit API authentication metadata. A model name or OAuth login does not prove API billing.
enum AgentAPIServiceDiscovery {
    static func provider(_ id: String, baseURL: String?) -> String? {
        if let baseURL {
            guard let url = URL(string: baseURL), url.scheme == "https", url.user == nil, url.password == nil else { return nil }
            switch url.host?.lowercased() {
            case "api.anthropic.com": return "Anthropic"
            case "api.openai.com": return "OpenAI"
            case "api.deepseek.com": return "DeepSeek"
            case "generativelanguage.googleapis.com": return "Google"
            case "api.x.ai": return "xAI"
            case "openrouter.ai": return "OpenRouter"
            case "api.groq.com": return "Groq"
            case "api.mistral.ai": return "Mistral"
            case "api.moonshot.cn", "api.moonshot.ai": return "Moonshot"
            case "api.z.ai", "open.bigmodel.cn": return "GLM"
            default: return nil
            }
        }
        switch id {
        case "anthropic": return "Anthropic"
        case "openai": return "OpenAI"
        case "deepseek": return "DeepSeek"
        case "google": return "Google"
        case "xai": return "xAI"
        case "openrouter": return "OpenRouter"
        case "groq": return "Groq"
        case "mistral": return "Mistral"
        case "moonshotai", "moonshotai-cn": return "Moonshot"
        case "zhipuai", "zai": return "GLM"
        default: return nil
        }
    }

    static func discover(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                         environment env: [String: String] = ProcessInfo.processInfo.environment) -> [AgentService] {
        let paths = OpenAgentPaths(home: home, environment: env)
        var services: [AgentService] = []
        func add(_ id: String, base: String?, key: String?, client: OpenAgentSource) {
            guard let key, !key.isEmpty, !key.hasPrefix("!"),
                  OpenAgentCredentials.service(provider: id, baseURL: base, client: client) == nil,
                  let provider = provider(id, baseURL: base) else { return }
            services.append(.init(client: client.name, provider: provider, product: .api))
        }

        let configRoot = URL(fileURLWithPath: env["XDG_CONFIG_HOME"] ?? home.appendingPathComponent(".config").path)
        let configs = ["opencode.json", "opencode.jsonc"].map { configRoot.appendingPathComponent("opencode/" + $0) }
            .filter { FileManager.default.fileExists(atPath: $0.path) }.map { OpenAgentCredentials.read($0, json5: true) }
        if configs.allSatisfy({ $0.objectValue != nil }) {
            for (id, auth) in OpenAgentCredentials.read(paths.openCode.appendingPathComponent("auth.json")).objectValue ?? [:]
                where auth["type"].stringValue == "api" {
                let base = configs.reversed().compactMap { $0["provider"][id]["options"]["baseURL"].stringValue }.first
                add(id, base: base, key: auth["key"].stringValue, client: .opencode)
            }
        }
        let piConfig = OpenAgentCredentials.read(paths.pi.appendingPathComponent("models.json"))["providers"]
        for (id, auth) in OpenAgentCredentials.read(paths.pi.appendingPathComponent("auth.json")).objectValue ?? [:]
            where auth["type"].stringValue == "api_key" {
            add(id, base: piConfig[id]["baseUrl"].stringValue, key: auth["key"].stringValue, client: .pi)
        }

        let claude = OpenAgentCredentials.read(home.appendingPathComponent(".claude/settings.json"))["env"]
        let base = env["ANTHROPIC_BASE_URL"] ?? claude["ANTHROPIC_BASE_URL"].stringValue
        let key = env["ANTHROPIC_API_KEY"] ?? env["ANTHROPIC_AUTH_TOKEN"]
            ?? claude["ANTHROPIC_API_KEY"].stringValue ?? claude["ANTHROPIC_AUTH_TOKEN"].stringValue
        if let key, !key.isEmpty,
           OpenAgentCredentials.service(provider: "", baseURL: base) == nil,
           let provider = provider("anthropic", baseURL: base) {
            services.append(.init(client: "Claude", provider: provider, product: .api))
        }
        return AgentService.merge([services])
    }
}
