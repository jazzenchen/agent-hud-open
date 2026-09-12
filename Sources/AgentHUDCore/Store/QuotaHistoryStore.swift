import Foundation

/// One quota reading, persisted so trends, burn rate and cap statistics survive restarts.
public struct QuotaSample: Hashable, Codable, Sendable {
    public let agentId: String
    public let timestamp: Date
    public let remainingPct: Double

    public init(agentId: String, timestamp: Date, remainingPct: Double) {
        self.agentId = agentId
        self.timestamp = timestamp
        self.remainingPct = remainingPct
    }
}

/// JSON-file backed sample store (Application Support). Keeps 30 days.
public actor QuotaHistoryStore {
    public static let retention: TimeInterval = 30 * 86400

    public static var defaultFileURL: URL {
        AppSupport.directory.appendingPathComponent("quota-history.json")
    }

    private let fileURL: URL?
    private var samples: [QuotaSample] = []

    /// - fileURL: nil keeps samples in memory only (tests).
    public init(fileURL: URL?) {
        self.fileURL = fileURL
        if let fileURL, let data = try? Data(contentsOf: fileURL) {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .secondsSince1970
            samples = (try? decoder.decode([QuotaSample].self, from: data)) ?? []
        }
    }

    public func append(_ new: [QuotaSample], now: Date) {
        guard !new.isEmpty else { return }
        samples.append(contentsOf: new)
        let cutoff = now.addingTimeInterval(-Self.retention)
        samples.removeAll { $0.timestamp < cutoff }
        samples.sort { $0.timestamp < $1.timestamp }
        save()
    }

    public func samples(agentId: String, since: Date) -> [QuotaSample] {
        samples.filter { $0.agentId == agentId && $0.timestamp >= since }
    }

    public var count: Int { samples.count }

    private func save() {
        guard let fileURL else { return }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(samples) else { return }
        try? FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: fileURL, options: .atomic)
    }
}
