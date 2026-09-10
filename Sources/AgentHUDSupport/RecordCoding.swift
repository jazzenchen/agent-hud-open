import Foundation
import CryptoKit

/// Deterministic encoding for persisted records and their identities.
public enum RecordCoding {
    private static let hex = Array("0123456789abcdef".utf8)

    public static func encoder() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        encoder.dateEncodingStrategy = .millisecondsSince1970
        return encoder
    }

    public static func milliseconds(_ date: Date) -> Int64 { Int64((date.timeIntervalSince1970 * 1000).rounded()) }
    public static func date(_ ms: Int64) -> Date { Date(timeIntervalSince1970: Double(ms) / 1000) }
    public static func hash(_ components: [String]) -> String {
        // Length-prefixed UTF-8 avoids delimiter ambiguity, independently of JSON encoder escaping.
        var bytes = Data()
        for value in components {
            let data = Data(value.utf8)
            bytes.append(contentsOf: "\(data.count):".utf8); bytes.append(data)
        }
        var result = [UInt8]()
        result.reserveCapacity(64)
        for byte in SHA256.hash(data: bytes) { result.append(hex[Int(byte >> 4)]); result.append(hex[Int(byte & 15)]) }
        return String(decoding: result, as: UTF8.self)
    }

}
