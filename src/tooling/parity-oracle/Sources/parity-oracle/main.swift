// Fills in `expected` for every case of every contract under the given directory, in English and Chinese, by calling
// agent-hud-native-open's own functions. Inputs are never changed. Run through `pnpm run parity:oracle`.
@testable import AgentHUDCore
import Foundation

struct OracleError: Error, CustomStringConvertible {
    let description: String
}

typealias Args = [String: Any]

func string(_ args: Args, _ key: String) -> String? { args[key] as? String }
func number(_ args: Args, _ key: String) -> Double? { (args[key] as? NSNumber)?.doubleValue }

func required<T>(_ value: T?, _ key: String) throws -> T {
    guard let value else { throw OracleError(description: "missing argument \(key)") }
    return value
}

func decimal(_ args: Args, _ key: String) throws -> Decimal {
    let text = try required(string(args, key), key)
    return try required(Decimal(string: text, locale: Locale(identifier: "en_US_POSIX")), key)
}

func date(_ args: Args, _ key: String) throws -> Date? {
    guard let text = string(args, key) else { return nil }
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return try required(formatter.date(from: text), key)
}

func calendar(_ args: Args) throws -> Calendar {
    var calendar = Calendar(identifier: .gregorian)
    let zone = string(args, "timeZone") ?? "UTC"
    calendar.timeZone = try required(TimeZone(identifier: zone), "timeZone")
    if let first = number(args, "firstWeekday") { calendar.firstWeekday = Int(first) }
    return calendar
}

func optional(_ value: String?) -> Any { value ?? NSNull() }

func evaluate(_ function: String, _ args: Args) throws -> Any {
    switch function {
    case "TokenFormat.short":
        return TokenFormat.short(Int(try required(number(args, "tokens"), "tokens")))
    case "TokenFormat.inOut":
        return TokenFormat.inOut(in: Int(try required(number(args, "input"), "input")),
                                 out: Int(try required(number(args, "output"), "output")))
    case "TokenFormat.percent":
        return TokenFormat.percent(try required(number(args, "value"), "value"))
    case "TokenFormat.percent1":
        return TokenFormat.percent1(try required(number(args, "value"), "value"))
    case "TokenFormat.share":
        return TokenFormat.share(Int(try required(number(args, "part"), "part")),
                                 of: Int(try required(number(args, "whole"), "whole")))
    case "Countdown.format":
        return Countdown.format(try required(number(args, "seconds"), "seconds"))
    case "Countdown.compact":
        return Countdown.compact(try required(number(args, "seconds"), "seconds"))
    case "Countdown.formatRough":
        return Countdown.formatRough(try required(number(args, "seconds"), "seconds"))
    case "Countdown.age":
        return Countdown.age(try required(number(args, "seconds"), "seconds"))
    case "Countdown.waited":
        return Countdown.waited(try required(number(args, "seconds"), "seconds"))
    case "Countdown.forecast":
        return Countdown.forecast(try required(number(args, "seconds"), "seconds"))
    case "Countdown.resetLabel":
        return Countdown.resetLabel(try date(args, "date"), now: try required(date(args, "now"), "now"),
                                    calendar: try calendar(args))
    case "Countdown.resetLabelCompact":
        return Countdown.resetLabelCompact(try date(args, "date"), now: try required(date(args, "now"), "now"),
                                           calendar: try calendar(args))
    case "MoneyFormat.amount":
        return MoneyFormat.amount(try decimal(args, "amount"), currency: try required(string(args, "currency"), "currency"),
                                  estimated: (args["estimated"] as? Bool) ?? false)
    case "MoneyFormat.short":
        return MoneyFormat.short(try decimal(args, "amount"), currency: try required(string(args, "currency"), "currency"))
    case "MoneyFormat.price":
        return MoneyFormat.price(try decimal(args, "amount"), currency: try required(string(args, "currency"), "currency"))
    case "SessionTitle.from":
        return optional(SessionTitle.from(try required(string(args, "text"), "text")))
    case "SessionTitle.named":
        return optional(SessionTitle.named(string(args, "text")))
    case "WindowNames.Period.shortName":
        return optional(WindowNames.Period(seconds: number(args, "seconds"))?.shortName)
    case "WindowNames.Period.afterWord":
        return optional(WindowNames.Period(seconds: number(args, "seconds"))?.afterWord)
    case "WindowNames.word":
        return WindowNames.word(try required(string(args, "name"), "name"))
    case "WindowNames.distinct":
        let names = try required(args["names"] as? [Any], "names").map { $0 as? String }
        return WindowNames.distinct(names).map(optional)
    case "WindowNames.leading":
        return WindowNames.leading(try required(string(args, "name"), "name"))
    case "CodexRateLimits.limitName":
        return CodexRateLimits.limitName(WindowNames.Period(seconds: number(args, "seconds")),
                                         secondary: (args["secondary"] as? Bool) ?? false)
    case "CopilotClient.names":
        let names = CopilotClient.names(try required(string(args, "key"), "key"), credits: (args["credits"] as? Bool) ?? false)
        return [names.full, names.short]
    case "OpenAgentQuotaClient.kimiName":
        let names = OpenAgentQuotaClient.kimiName(WindowNames.Period(seconds: number(args, "seconds")))
        return [names.full, names.short]
    case "OpenAgentQuotaClient.glmName":
        let names = OpenAgentQuotaClient.glmName(try required(string(args, "type"), "type"),
                                                 period: WindowNames.Period(seconds: number(args, "seconds")),
                                                 monthly: (args["monthly"] as? Bool) ?? false)
        return [names.full, names.short]
    case "AntigravityClient.groupWord":
        return AntigravityClient.groupWord(try required(string(args, "name"), "name"))
    case "VendorCatalog.window":
        return VendorCatalog.window(try required(string(args, "value"), "value"), vendor: try required(string(args, "vendor"), "vendor"))
    case "VendorCatalog.windowWord":
        return VendorCatalog.windowWord(try required(string(args, "value"), "value"), vendor: try required(string(args, "vendor"), "vendor"))
    case "AccountObservation.planLabel":
        // This repository's provider ids name the clients the Swift app calls Claude and Codex.
        let provider = try required(string(args, "provider"), "provider")
        let source = provider == "claude" ? "claude-code" : provider == "codex" ? "codex-cli" : provider
        let plan = try required(string(args, "plan"), "plan")
        return optional(SourceStatus(id: source, name: provider, detail: "", state: .ready(plan: plan)).planLabel)
    case "ClaudeModelInfo.displayName":
        return optional(ClaudeModelInfo.parse(try required(string(args, "model"), "model"))?.displayName)
    case "ModelCatalog.consumerName":
        return ModelCatalog.consumerName(of: try required(string(args, "agentId"), "agentId"))
    case "SessionSource.vendor":
        return optional(SessionSource.vendor(impliedBy: try required(string(args, "agentId"), "agentId")))
    case "QuotaForecast.text":
        let outlook: QuotaOutlook = switch try required(string(args, "outlook"), "outlook") {
        case "untimed": .untimed
        case "exhausted": .exhausted
        case "noEstimate": .noEstimate
        case "noUsage": .noUsage
        case "insufficientData": .insufficientData
        default: .exhausts(in: try required(number(args, "seconds"), "seconds"), beforeReset: (args["beforeReset"] as? Bool) ?? true)
        }
        return optional(QuotaForecast.text(of: outlook, projectedUsedAtReset: number(args, "projectedUsedAtReset")))
    case "QuotaForecast.byReset":
        return QuotaForecast.byReset(try required(number(args, "projectedUsedPercent"), "projectedUsedPercent"))
    case "QuotaSampling.label", "QuotaSampling.hint":
        let sampling = QuotaSampling(elapsed: try required(number(args, "elapsed"), "elapsed"),
                                     required: try required(number(args, "required"), "required"),
                                     afterReset: (args["afterReset"] as? Bool) ?? false)
        return function == "QuotaSampling.label" ? sampling.label : sampling.hint
    default:
        throw OracleError(description: "no oracle for \(function)")
    }
}

let languages: [(key: String, language: AppLanguage)] = [("en", .en), ("zh", .zhHans)]
let directory = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "src/fixtures/parity/contracts")
let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
    .filter { $0.pathExtension == "json" }
    .sorted { $0.lastPathComponent < $1.lastPathComponent }
var failures = 0
for file in files {
    var contract = try JSONSerialization.jsonObject(with: Data(contentsOf: file)) as! [String: Any]
    var cases = contract["cases"] as! [[String: Any]]
    for index in cases.indices {
        let function = cases[index]["fn"] as! String
        let args = cases[index]["args"] as? Args ?? [:]
        var expected: [String: Any] = [:]
        do {
            for (key, language) in languages {
                L10n.setLanguage(language)
                expected[key] = try evaluate(function, args)
            }
            cases[index]["expected"] = expected
        } catch {
            failures += 1
            FileHandle.standardError.write(Data("\(file.lastPathComponent) \(cases[index]["name"] ?? "?"): \(error)\n".utf8))
        }
    }
    contract["cases"] = cases
    let data = try JSONSerialization.data(withJSONObject: contract,
                                          options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
    try (data + Data("\n".utf8)).write(to: file)
    print("\(file.lastPathComponent): \(cases.count) cases")
}
exit(failures == 0 ? 0 : 1)
