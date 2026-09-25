import Foundation

/// The vendors' published list prices, per million tokens. Cache writes cost the input rate unless a list prices them
/// apart; tiers start at the prompt length the vendor names.
extension ModelCatalog {
    static let models: [String: Model] = {
        var models: [String: Model] = [
            // Anthropic and OpenAI sell one list worldwide.
            "claude-fable-5-1": claude("10", "50", read: "0.25", window: 1_000_000),
            "claude-mythos-5-1": claude("10", "50", read: "0.25", window: 1_000_000),
            "claude-fable-5": claude("10", "50", window: 1_000_000),
            "claude-mythos-5": claude("10", "50", window: 1_000_000),
            "claude-opus-5-5": claude("4", "20", read: "0.20", window: 1_000_000),
            "claude-opus-5": claude("5", "25", window: 1_000_000),
            "claude-opus-4-8": claude("5", "25", window: 1_000_000),
            "claude-opus-4-7": claude("5", "25", window: 1_000_000),
            "claude-opus-4-6": claude("5", "25", window: 1_000_000),
            "claude-opus-4-5": claude("5", "25", window: 200_000),
            "claude-opus-4-1": claude("15", "75", window: 200_000),
            "claude-opus-4": claude("15", "75", window: 200_000),
            "claude-opus-4-0": claude("15", "75", window: 200_000),
            "claude-sonnet-5": claude("2", "10", window: 1_000_000),
            "claude-sonnet-4-6": claude("3", "15", window: 1_000_000),
            "claude-sonnet-4-5": claude("3", "15", window: 200_000),
            "claude-sonnet-4": claude("3", "15", window: 200_000),
            "claude-sonnet-4-0": claude("3", "15", window: 200_000),
            "claude-haiku-4-5": claude("1", "5", window: 200_000),
            "claude-3-5-haiku": claude("0.80", "4", window: 200_000),
            "gpt-6-astra": openAI("10", "1", "50"),
            "gpt-6-sol": openAI("2", "0.20", "10"),
            "gpt-6-luna": openAI("0.10", "0.01", "0.50"),
            "gpt-5.6-sol": openAI("4", "0.40", "20"),
            "gpt-5.6-terra": openAI("2", "0.20", "12"),
            "gpt-5.6-luna": openAI("0.20", "0.02", "1.20"),
            "gpt-5.5": openAI("5", "0.50", "30"),
            "gpt-5.4": openAI("2.50", "0.25", "15"),
            "gpt-5.3-codex": openAI("1.75", "0.175", "14", long: false),

            // DeepSeek bills its account's currency at off-peak rates, twice those in peak hours.
            "deepseek-flash": regional("DeepSeek", international: flat("0.15", "0.003", "0.60"), china: flat("1", "0.02", "4"),
                                       window: 1_000_000, peak: true),
            "deepseek-v4-pro": regional("DeepSeek", international: flat("0.66", "0.022", "1.98"), china: flat("4.5", "0.15", "13.5"),
                                        window: 1_000_000, peak: true),

            // xAI: from 200K prompt tokens every token of the call costs twice.
            "grok-4.7": xAI("2.00", "0.50", "6.00", window: 500_000),
            "grok-4.6": xAI("2.00", "0.50", "6.00", window: 500_000),
            "grok-4.5": xAI("2.00", "0.30", "6.00", window: 500_000),
            "grok-4.3": xAI("1.25", "0.20", "2.50", window: 1_000_000),
            "grok-4.20-0309-reasoning": xAI("1.25", "0.20", "2.50", window: 1_000_000),
            "grok-4.20-0309-non-reasoning": xAI("1.25", "0.20", "2.50", window: 1_000_000),
            "grok-4.20-multi-agent-0309": xAI("1.25", "0.20", "2.50", window: 1_000_000),
            "grok-build-0.1": xAI("1.00", "0.20", "2.00", window: 256_000),

            // Google's paid tier, Standard. Gemini 3.8 Flash's list doubles on 2027-01-01.
            "gemini-3.8-flash": google("0.75", "0.075", "3.75"),
            "gemini-3.7-flash": google("0.75", "0.075", "3.75"),
            "gemini-3.6-flash": google("0.75", "0.075", "3.75"),
            "gemini-3.5-flash": google("1.50", "0.15", "9.00"),
            "gemini-3.5-flash-lite": google("0.30", "0.03", "2.50"),
            "gemini-3.1-flash-lite": google("0.25", "0.025", "1.50"),
            "gemini-3-flash-preview": google("0.50", "0.05", "3.00"),
            "gemini-3.1-pro-preview": google("2.00", "0.20", "12.00", long: rates("4.00", "0.40", "18.00")),

            // Zhipu: Z.ai's list abroad, BigModel's in China, where the larger models cost more from 32K prompt tokens.
            // BigModel's shortest tier also depends on the output length; replies here are longer than its 200-token line.
            "glm-5.3": glm(flat("1.4", "0.26", "4.4"), china: flat("8", "2", "28"), window: 1_000_000),
            "glm-5.3-flash": glm(flat("0.15", "0.03", "0.50"), china: flat("0.8", "0.23", "2.8"), window: 1_000_000),
            "glm-5.3-flashx": glm(flat("0.37", "0.075", "1.25"), china: flat("2", "0.57", "7"), window: 1_000_000),
            "glm-5.2": glm(flat("1.4", "0.26", "4.4"), china: flat("8", "2", "28"), window: 1_000_000),
            "glm-5.1": glm(flat("1.4", "0.26", "4.4"), china: tiered(("6", "1.3", "24"), (32_000, "8", "2", "28")), window: 200_000),
            "glm-5-turbo": glm(nil, china: tiered(("5", "1.2", "22"), (32_000, "7", "1.8", "26")), window: 200_000),
            "glm-5": glm(flat("1", "0.2", "3.2"), china: tiered(("4", "1", "18"), (32_000, "6", "1.5", "22")), window: 200_000),
            "glm-4.7": glm(flat("0.6", "0.11", "2.2"), china: tiered(("3", "0.6", "14"), (32_000, "4", "0.8", "16")), window: 200_000),
            "glm-4.6": glm(flat("0.6", "0.11", "2.2"), china: nil, window: 200_000),
            "glm-4.7-flashx": glm(flat("0.07", "0.01", "0.4"), china: flat("0.5", "0.1", "3"), window: 200_000),
            "glm-4.7-flash": glm(flat("0", "0", "0"), china: nil, window: 200_000),
            "glm-4.5": glm(flat("0.6", "0.11", "2.2"), china: nil, window: 128_000),
            "glm-4.5-x": glm(flat("2.2", "0.45", "8.9"), china: nil, window: 128_000),
            "glm-4.5-air": glm(flat("0.2", "0.03", "1.1"), china: tiered(("0.8", "0.16", "6"), (32_000, "1.2", "0.24", "8")), window: 128_000),
            "glm-4.5-airx": glm(flat("1.1", "0.22", "4.5"), china: nil, window: 128_000),
            "glm-4.5-flash": glm(flat("0", "0", "0"), china: nil, window: 128_000),

            // Moonshot: Kimi K3 writes its five-minute cache at the input rate.
            "kimi-k3": kimi(flat("3.00", "0.30", "15.00"), china: flat("20", "2", "100"), window: 1_048_576),
            "kimi-k2.7-code": kimi(flat("0.95", "0.19", "4.00"), china: flat("6.50", "1.30", "27"), window: 262_144),
            "kimi-k2.7-code-highspeed": kimi(flat("1.90", "0.38", "8.00"), china: flat("13", "2.60", "54"), window: 262_144),
            "kimi-k2.6": kimi(flat("0.95", "0.16", "4.00"), china: flat("6.50", "1.10", "27"), window: 262_144),

            // Alibaba Model Studio: Singapore abroad, Beijing in China. Tiers follow the call's prompt; a cache read costs
            // the implicit-cache rate and a cache write the explicit creation rate, except where a model has explicit
            // caching only.
            "qwen3-coder-plus": qwen(tiered(("1", "0.2", "5", write: "1.25"), (32_001, "1.8", "0.36", "9", write: "2.25"),
                                            (128_001, "3", "0.6", "15", write: "3.75"), (256_001, "6", "1.2", "60", write: "7.5")),
                                     china: tiered(("4", "0.8", "16", write: "5"), (32_001, "6", "1.2", "24", write: "7.5"),
                                                   (128_001, "10", "2", "40", write: "12.5"), (256_001, "20", "4", "200", write: "25")),
                                     window: 1_000_000),
            "qwen3-coder-flash": qwen(tiered(("0.3", "0.06", "1.5", write: "0.375"), (32_001, "0.5", "0.1", "2.5", write: "0.625"),
                                             (128_001, "0.8", "0.16", "4", write: "1"), (256_001, "1.6", "0.32", "9.6", write: "2")),
                                      china: tiered(("1", "0.2", "4", write: "1.25"), (32_001, "1.5", "0.3", "6", write: "1.875"),
                                                    (128_001, "2.5", "0.5", "10", write: "3.125"), (256_001, "5", "1", "25", write: "6.25")),
                                      window: 1_000_000),
            "qwen3-coder-next": qwen(tiered(("0.3", "0.3", "1.5"), (32_001, "0.5", "0.5", "2.5"), (128_001, "0.8", "0.8", "4")),
                                     china: tiered(("1", "1", "4"), (32_001, "1.5", "1.5", "6"), (128_001, "2.5", "2.5", "10")),
                                     window: 262_144),
            "qwen3-max": qwen(tiered(("1.2", "0.24", "6", write: "1.5"), (32_001, "2.4", "0.48", "12", write: "3"),
                                     (128_001, "3", "0.6", "15", write: "3.75")),
                              china: tiered(("2.5", "0.5", "10", write: "3.125"), (32_001, "4", "0.8", "16", write: "5"),
                                            (128_001, "7", "1.4", "28", write: "8.75")),
                              window: 262_144),
            "qwen3.6-max-preview": qwen(tiered(("1.3", "0.13", "7.8", write: "1.625"), (128_001, "2", "0.2", "12", write: "2.5")),
                                        china: tiered(("9", "0.9", "54", write: "11.25"), (128_001, "15", "1.5", "90", write: "18.75")),
                                        window: 262_144),
            "qwen3.6-plus": qwen(tiered(("0.5", "0.05", "3", write: "0.625"), (256_001, "2", "0.2", "6", write: "2.5")),
                                 china: tiered(("2", "0.2", "12", write: "2.5"), (256_001, "8", "0.8", "48", write: "10")),
                                 window: 1_000_000),
            "qwen3.6-flash": qwen(tiered(("0.25", "0.025", "1.5", write: "0.3125"), (256_001, "1", "0.1", "4", write: "1.25")),
                                  china: tiered(("1.2", "0.12", "7.2", write: "1.5"), (256_001, "4.8", "0.48", "28.8", write: "6")),
                                  window: 1_000_000),
            "qwen3.6-35b-a3b": qwen(flat("0.375", "0.375", "2.25"), china: flat("1.8", "1.8", "10.8"), window: 262_144),
            "qwen3.6-27b": qwen(flat("0.6", "0.6", "3.6"), china: flat("3", "3", "18"), window: 262_144),
            "qwen3.8-max": qwen(flat("2", "0.25", "6", write: "2.5"), china: flat("12", "1.5", "36", write: "15"), window: 1_000_000),
            "qwen3.7-max": qwen(flat("2.5", "0.5", "7.5", write: "3.125"), china: flat("12", "2.4", "36", write: "15"), window: 1_000_000),
            "qwen3.7-plus": qwen(tiered(("0.4", "0.08", "1.6", write: "0.5"), (256_001, "1.2", "0.24", "4.8", write: "1.5")),
                                 china: tiered(("2", "0.4", "8", write: "2.5"), (256_001, "6", "1.2", "24", write: "7.5")),
                                 window: 1_000_000),
            "qwen3.8-flash": qwen(flat("0.15", "0.016", "0.47", write: "0.2"), china: flat("0.8", "0.1", "2.7", write: "1.25"),
                                  window: 1_000_000),
            "qwen3.7-flash": qwen(tiered(("0.03", "0.006", "0.13", write: "0.038"), (32_001, "0.1", "0.02", "0.4", write: "0.125"),
                                         (256_001, "0.2", "0.04", "0.8", write: "0.25")),
                                  china: tiered(("0.2", "0.04", "0.8", write: "0.25"), (32_001, "0.6", "0.12", "2.4", write: "0.75"),
                                                (256_001, "1.2", "0.24", "4.8", write: "1.5")),
                                  window: 1_000_000),

            // MiniMax lists US dollars only, at its permanent half price.
            "minimax-m3": global("MiniMax", rates("0.30", "0.06", "1.20"), long: (512_001, rates("0.60", "0.12", "2.40")), window: 1_000_000),
            "minimax-m2.7": global("MiniMax", rates("0.3", "0.06", "1.2", write: "0.375"), window: 204_800),
            "minimax-m2.7-highspeed": global("MiniMax", rates("0.6", "0.06", "2.4", write: "0.375"), window: 204_800),
            "minimax-m2.5": global("MiniMax", rates("0.3", "0.03", "1.2", write: "0.375"), window: 204_800),
            "minimax-m2.5-highspeed": global("MiniMax", rates("0.6", "0.03", "2.4", write: "0.375"), window: 204_800),
            "minimax-m2.1": global("MiniMax", rates("0.3", "0.03", "1.2", write: "0.375"), window: 204_800),
            "minimax-m2.1-highspeed": global("MiniMax", rates("0.6", "0.03", "2.4", write: "0.375"), window: 204_800),
            "minimax-m2": global("MiniMax", rates("0.3", "0.03", "1.2", write: "0.375"), window: 204_800),

            // Xiaomi MiMo writes its cache free for now.
            "mimo-v2.6-pro": mimo(flat("0.435", "0.0036", "0.87", write: "0"), china: flat("3", "0.025", "6", write: "0")),
            "mimo-v2.6-flash": mimo(flat("0.14", "0.0028", "0.28", write: "0"), china: flat("1", "0.02", "2", write: "0")),
            "mimo-v2.6-pro-ultraspeed": mimo(flat("4.35", "0.036", "8.7", write: "0"), china: flat("30", "0.25", "60", write: "0")),
        ]
        // Names the vendors still accept for the models above.
        let aliases = [
            "deepseek-v4-flash": "deepseek-flash", "deepseek-v4-flash-vision-exp": "deepseek-flash",
            "grok-4.5-latest": "grok-4.5", "grok-build-latest": "grok-4.5", "grok-4.3-latest": "grok-4.3",
            "grok-4.20": "grok-4.20-0309-reasoning", "grok-4.20-reasoning": "grok-4.20-0309-reasoning",
            "grok-4.20-reasoning-latest": "grok-4.20-0309-reasoning",
            "grok-4.20-non-reasoning": "grok-4.20-0309-non-reasoning", "grok-4.20-non-reasoning-latest": "grok-4.20-0309-non-reasoning",
            "grok-4.20-multi-agent": "grok-4.20-multi-agent-0309", "grok-4.20-multi-agent-latest": "grok-4.20-multi-agent-0309",
            "grok-code-fast-1": "grok-build-0.1", "grok-code-fast": "grok-build-0.1", "grok-code-fast-1-0825": "grok-build-0.1",
            "gemini-3.1-pro-preview-customtools": "gemini-3.1-pro-preview",
            "qwen3.8-max-0902": "qwen3.8-max",
            "mimo-v2.5-pro": "mimo-v2.6-pro", "mimo-v2.5": "mimo-v2.6-flash",
        ]
        for (alias, name) in aliases { models[alias] = models[name] }
        return models
    }()

    private static func decimal(_ text: String) -> Decimal { Decimal(string: text, locale: Locale(identifier: "en_US_POSIX"))! }

    private static func rates(_ input: String, _ read: String, _ output: String, write: String? = nil) -> Rates {
        Rates(input: decimal(input), cacheWrite: decimal(write ?? input), cacheRead: decimal(read), output: decimal(output))
    }

    private static func flat(_ input: String, _ read: String, _ output: String, write: String? = nil) -> Price {
        Price(rates: rates(input, read, output, write: write), tiers: [])
    }

    private static func tiered(_ base: (String, String, String), _ tiers: (Int, String, String, String)...) -> Price {
        Price(rates: rates(base.0, base.1, base.2), tiers: tiers.map { .init(from: $0.0, rates: rates($0.1, $0.2, $0.3)) })
    }

    private static func tiered(_ base: (String, String, String, write: String), _ tiers: (Int, String, String, String, write: String)...) -> Price {
        Price(rates: rates(base.0, base.1, base.2, write: base.write),
              tiers: tiers.map { .init(from: $0.0, rates: rates($0.1, $0.2, $0.3, write: $0.write)) })
    }

    /// Claude Code writes its prompt cache with the one-hour lifetime, billed at twice the input rate. Cache reads cost a
    /// tenth of input unless the model has its own rate.
    private static func claude(_ input: String, _ output: String, read: String? = nil, window: Int) -> Model {
        let input = decimal(input)
        let rates = Rates(input: input, cacheWrite: input * 2, cacheRead: read.map(decimal) ?? input / 10, output: decimal(output))
        return Model(vendor: "Anthropic", prices: [.international: Price(rates: rates, tiers: [])], global: true,
                     contextWindow: window, peakHours: false)
    }

    /// Prompts over 272K tokens cost twice the input and cache rates and one and a half times the output rate on the
    /// models that accept them.
    private static func openAI(_ input: String, _ read: String, _ output: String, long: Bool = true) -> Model {
        let base = rates(input, read, output)
        let longRates = Rates(input: base.input * 2, cacheWrite: base.cacheWrite * 2, cacheRead: base.cacheRead * 2,
                              output: base.output * 3 / 2)
        return Model(vendor: "OpenAI", prices: [.international: Price(rates: base, tiers: long ? [.init(from: 272_001, rates: longRates)] : [])],
                     global: true, contextWindow: nil, peakHours: false)
    }

    private static func xAI(_ input: String, _ read: String, _ output: String, window: Int) -> Model {
        let base = rates(input, read, output)
        let long = Rates(input: base.input * 2, cacheWrite: base.cacheWrite * 2, cacheRead: base.cacheRead * 2, output: base.output * 2)
        return global("xAI", base, long: (200_000, long), window: window)
    }

    private static func google(_ input: String, _ read: String, _ output: String, long: Rates? = nil) -> Model {
        global("Google", rates(input, read, output), long: long.map { (from: 200_001, rates: $0) }, window: 1_048_576)
    }

    private static func global(_ vendor: String, _ base: Rates, long: (from: Int, rates: Rates)? = nil, window: Int?) -> Model {
        Model(vendor: vendor, prices: [.international: Price(rates: base, tiers: long.map { [.init(from: $0.from, rates: $0.rates)] } ?? [])],
              global: true, contextWindow: window, peakHours: false)
    }

    private static func regional(_ vendor: String, international: Price?, china: Price?, window: Int?, peak: Bool = false) -> Model {
        var prices: [Region: Price] = [:]
        prices[.international] = international
        prices[.china] = china
        return Model(vendor: vendor, prices: prices, global: false, contextWindow: window, peakHours: peak)
    }

    private static func glm(_ international: Price?, china: Price?, window: Int) -> Model {
        regional("GLM", international: international, china: china, window: window)
    }

    private static func kimi(_ international: Price, china: Price, window: Int) -> Model {
        regional("Moonshot", international: international, china: china, window: window)
    }

    private static func qwen(_ international: Price, china: Price, window: Int) -> Model {
        regional("Qwen", international: international, china: china, window: window)
    }

    private static func mimo(_ international: Price, china: Price) -> Model {
        regional("MiMo", international: international, china: china, window: 1_000_000)
    }
}
