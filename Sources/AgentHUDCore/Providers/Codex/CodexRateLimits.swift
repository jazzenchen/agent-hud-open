import Foundation

/// The `account/rateLimits/read` fields used by the HUD.
public struct CodexRateLimits: Decodable, Sendable {
    public struct Window: Decodable, Sendable {
        public let usedPercent: Double
        public let windowDurationMins: Int?
        public let resetsAt: TimeInterval?

        public var remainingPct: Double { max(0, min(100, 100 - usedPercent)) }
        public var resetAt: Date? { resetsAt.map(Date.init(timeIntervalSince1970:)) }
        public var duration: TimeInterval? { windowDurationMins.map { Double($0) * 60 } }
    }

    public struct Bucket: Decodable, Sendable {
        public let limitId: String?
        public let limitName: String?
        public let primary: Window?
        public let secondary: Window?
        public let planType: String?
    }

    public struct Row: Sendable {
        public let id: String
        public let label: String
        public let window: Window
        public let weekly: Window?
        public var account: ProviderAccount? = nil

        public var descriptor: AgentDescriptor {
            AgentDescriptor(id: id, vendor: "Codex", model: label, source: L10n.sourceCodexAppServer, enabled: true, account: account)
        }

        func scoped(to account: ProviderAccount) -> Row {
            Row(id: account.windowID(id), label: label, window: window, weekly: weekly, account: account)
        }
    }

    /// The `account/read` result from the same engine process.
    public struct SignedInAccount: Decodable, Sendable {
        public let type: String?
        public let email: String?
        public let planType: String?
    }

    public let rateLimits: Bucket?
    public let rateLimitsByLimitId: [String: Bucket]?
    public let rateLimitResetCredits: CodexResetCredits?
    /// The ChatGPT workspace of this snapshot. Members of one workspace share it, so the email separates users.
    public let accountId: String?
    public var account: SignedInAccount?

    /// A present multi-bucket map is authoritative, including an empty map.
    public var buckets: [(id: String, bucket: Bucket)] {
        if let map = rateLimitsByLimitId {
            return map.keys.sorted { a, b in
                if a == "codex" { return b != "codex" }
                if b == "codex" { return false }
                return a < b
            }.map { ($0, map[$0]!) }
        }
        return rateLimits.map { [($0.limitId ?? "codex", $0)] } ?? []
    }

    public var plan: String? { buckets.compactMap { $0.bucket.planType }.first ?? account?.planType }

    public func providerAccount(home: String) -> ProviderAccount {
        ProviderAccount.identified(provider: "Codex", user: account?.email?.lowercased(), workspace: accountId)
            ?? .unresolved(provider: "Codex", home: home)
    }

    /// The key the same account got from a reading without its email, as when `account/read` answered too late. Nil
    /// unless this reading has both the email and the workspace.
    public var keyWithoutEmail: String? {
        guard account?.email?.isEmpty == false else { return nil }
        return ProviderAccount.identified(provider: "Codex", user: nil, workspace: accountId)?.id
    }

    /// Window rows keyed by their own window id (`codex`, `codex:<limit>:<slot>`); providers scope them to the account.
    public func rows(home: String) -> [Row] {
        let account = providerAccount(home: home)
        return rows.map { $0.scoped(to: account) }
    }

    public var rows: [Row] {
        buckets.flatMap { id, bucket in
            let weekly = [bucket.primary, bucket.secondary].compactMap { $0 }.first { $0.windowDurationMins == 10080 }
            return [("primary", bucket.primary), ("secondary", bucket.secondary)].compactMap { slot, window -> Row? in
                guard let window else { return nil }
                let period: String
                switch window.windowDurationMins {
                case 10080: period = L10n.text("本周", "Weekly")
                case .some(let minutes) where minutes > 0 && minutes % 60 == 0:
                    period = "\(minutes / 60)h"
                case .some(let minutes) where minutes > 0: period = "\(minutes)m"
                default: period = L10n.text(slot == "primary" ? "主额度" : "次额度", slot.capitalized)
                }
                let name = id == "codex" ? nil : VendorCatalog.window(bucket.limitName ?? id, vendor: "Codex")
                // The shared primary window keeps the old placeholder's id as its window key, preserving preferences.
                let rowId = id == "codex" && slot == "primary" ? "codex" : "codex:\(id):\(slot)"
                return Row(id: rowId, label: name.map { "\($0) · \(period)" } ?? period, window: window, weekly: weekly)
            }
        }
    }
}

/// Account-wide earned resets. The count is authoritative; credit details can be absent or capped.
public struct CodexResetCredits: Codable, Hashable, Sendable {
    public struct Credit: Codable, Hashable, Sendable, Identifiable {
        public let id: String
        public let expiresAt: TimeInterval?

        public init(id: String, expiresAt: TimeInterval?) {
            self.id = id
            self.expiresAt = expiresAt
        }

        public var expirationDate: Date? { expiresAt.map(Date.init(timeIntervalSince1970:)) }
    }

    public let availableCount: Int
    public let credits: [Credit]?

    public init(availableCount: Int, credits: [Credit]?) {
        self.availableCount = availableCount
        self.credits = credits
    }

    public var creditsByExpiry: [Credit] {
        (credits ?? []).sorted { ($0.expiresAt ?? .infinity) < ($1.expiresAt ?? .infinity) }
    }
}

public enum CodexLocator {
    public static var dataDirectory: URL {
        dataDirectory(home: FileManager.default.homeDirectoryForCurrentUser)
    }

    static func dataDirectory(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let path = environment["CODEX_HOME"], !path.isEmpty {
            return URL(fileURLWithPath: path, isDirectory: true)
        }
        return home.appendingPathComponent(".codex", isDirectory: true)
    }

    /// Prefer the self-contained Desktop engine; GUI PATH often cannot run npm's node shim. The app is looked for under
    /// its known names, then wherever Launch Services has it under its bundle ID, whatever it is called now.
    public static func candidates(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                  applications: URL = URL(fileURLWithPath: "/Applications"),
                                  path: String = ProcessInfo.processInfo.environment["PATH"] ?? "",
                                  registered: [URL] = VendorCatalog.applications("Codex")) -> [URL] {
        desktop(home: home, applications: applications) + registered.map(engine(in:)) + cli(home: home, path: path)
    }

    /// The same order as `candidates`, asking Launch Services only when no app sits under a known name.
    public static func find(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                            applications: URL = URL(fileURLWithPath: "/Applications"),
                            path: String = ProcessInfo.processInfo.environment["PATH"] ?? "",
                            registered: @autoclosure () -> [URL] = VendorCatalog.applications("Codex")) -> URL? {
        let runnable = { (url: URL) in FileManager.default.isExecutableFile(atPath: url.path) }
        return desktop(home: home, applications: applications).first(where: runnable)
            ?? registered().map(engine(in:)).first(where: runnable)
            ?? cli(home: home, path: path).first(where: runnable)
    }

    private static func engine(in app: URL) -> URL { app.appendingPathComponent("Contents/Resources/codex") }

    private static func desktop(home: URL, applications: URL) -> [URL] {
        [applications, home.appendingPathComponent("Applications")].flatMap { root in
            ["Codex.app", "ChatGPT.app"].map { engine(in: root.appendingPathComponent($0)) }
        }
    }

    private static func cli(home: URL, path: String) -> [URL] {
        [home.appendingPathComponent(".bun/bin/codex"), home.appendingPathComponent(".local/bin/codex"),
         URL(fileURLWithPath: "/opt/homebrew/bin/codex"), URL(fileURLWithPath: "/usr/local/bin/codex")]
            + path.split(separator: ":").map { URL(fileURLWithPath: String($0)).appendingPathComponent("codex") }
    }
}
