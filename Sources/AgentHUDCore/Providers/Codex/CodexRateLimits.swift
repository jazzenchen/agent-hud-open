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

        public var descriptor: AgentDescriptor {
            AgentDescriptor(id: id, vendor: "Codex", model: label, source: L10n.sourceCodexAppServer, enabled: true)
        }
    }

    public let rateLimits: Bucket?
    public let rateLimitsByLimitId: [String: Bucket]?
    public let rateLimitResetCredits: CodexResetCredits?

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

    public var plan: String? { buckets.compactMap { $0.bucket.planType }.first }

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
                let name = id == "codex" ? nil : (bucket.limitName ?? id)
                // Keep the old Codex placeholder's id for the shared primary window, preserving preferences.
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
        if let path = ProcessInfo.processInfo.environment["CODEX_HOME"], !path.isEmpty {
            return URL(fileURLWithPath: path, isDirectory: true)
        }
        return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex", isDirectory: true)
    }

    public static func candidates(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                  applications: URL = URL(fileURLWithPath: "/Applications"),
                                  path: String = ProcessInfo.processInfo.environment["PATH"] ?? "") -> [URL] {
        // Prefer the self-contained Desktop engine; GUI PATH often cannot run npm's node shim.
        let desktop = [applications, home.appendingPathComponent("Applications")].flatMap { root in
            ["Codex.app", "ChatGPT.app"].map { root.appendingPathComponent("\($0)/Contents/Resources/codex") }
        }
        let cli = [home.appendingPathComponent(".bun/bin/codex"), home.appendingPathComponent(".local/bin/codex"),
                   URL(fileURLWithPath: "/opt/homebrew/bin/codex"), URL(fileURLWithPath: "/usr/local/bin/codex")]
        return desktop + cli + path.split(separator: ":").map { URL(fileURLWithPath: String($0)).appendingPathComponent("codex") }
    }

    public static func find(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                            applications: URL = URL(fileURLWithPath: "/Applications"),
                            path: String = ProcessInfo.processInfo.environment["PATH"] ?? "") -> URL? {
        candidates(home: home, applications: applications, path: path).first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }
}
