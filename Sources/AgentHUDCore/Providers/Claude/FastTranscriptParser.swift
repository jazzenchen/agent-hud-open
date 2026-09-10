import Foundation

/// Byte-level transcript reader. Transcript lines can be hundreds of KB (tool results), so instead of decoding each
/// line as JSON it extracts only the fields the accumulator needs with `memchr`/`memmem` searches. The small
/// `usage` object is the only piece handed to `JSONSerialization`.
///
/// Claude Code puts `message.role`/`message.id`/`model` at the start of a line and `stop_reason`, `usage`,
/// `requestId`, `timestamp` at the end, with the (possibly huge) message content in between. Early keys are
/// searched in a bounded prefix and late keys backwards, so a 500 KB tool result costs almost nothing.
/// Current builds write `type`, `cwd` and `sessionId` after the message; older builds wrote them first.
public enum FastTranscriptParser {
    private static let prefixLength = 2048
    /// Current builds write `entrypoint`, `cwd`, `sessionId` and `version` after the message, close to the end.
    private static let tailLength = 2048

    private static let timestampKey = Array("\"timestamp\":\"".utf8)
    private static let assistantRoleMarker = Array("\"role\":\"assistant\"".utf8)
    private static let userRoleMarker = Array("\"role\":\"user\"".utf8)
    private static let assistantMarker = Array("\"type\":\"assistant\"".utf8)
    private static let userMarker = Array("\"type\":\"user\"".utf8)
    private static let sidechainMarker = Array("\"isSidechain\":true".utf8)
    private static let metaMarker = Array("\"isMeta\":true".utf8)
    private static let stopReasonKey = Array("\"stop_reason\":".utf8)
    private static let usageKey = Array("\"usage\":{".utf8)
    private static let modelKey = Array("\"model\":\"".utf8)
    private static let messageIdKey = Array("\"message\":{\"id\":\"".utf8)
    private static let fallbackIdKey = Array("\"id\":\"msg_".utf8)
    private static let requestIdKey = Array("\"requestId\":\"".utf8)
    private static let sessionIdKey = Array("\"sessionId\":\"".utf8)
    private static let cwdKey = Array("\"cwd\":\"".utf8)
    private static let entrypointKey = Array("\"entrypoint\":\"".utf8)
    private static let contentStringKey = Array("\"content\":\"".utf8)
    private static let contentTextKey = Array("\"content\":[{\"type\":\"text\",\"text\":\"".utf8)
    // Block keys are not written in a fixed order (`tool_use_id` usually precedes `type`), so match the type alone.
    private static let toolResultMarker = Array("\"type\":\"tool_result\"".utf8)

    /// Parses complete lines in `data` (a trailing partial line is ignored by the caller).
    public static func parse(_ data: Data) -> [TranscriptEvent] {
        var events: [TranscriptEvent] = []
        data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            guard let base = buffer.baseAddress else { return }
            var start = 0
            let count = buffer.count
            while start < count {
                let rest = base + start
                let remaining = count - start
                let end: Int
                if let found = memchr(rest, 0x0A, remaining) {
                    end = start + rest.distance(to: found)
                } else {
                    end = count
                }
                if end > start, let event = parseLine(UnsafeRawBufferPointer(start: rest, count: end - start)) {
                    events.append(event)
                }
                start = end + 1
            }
        }
        return events
    }

    /// Convenience for tests and small inputs.
    public static func parseLine(_ line: Data) -> TranscriptEvent? {
        line.withUnsafeBytes { parseLine($0) }
    }

    static func parseLine(_ line: UnsafeRawBufferPointer) -> TranscriptEvent? {
        guard line.count > 2, line[0] == UInt8(ascii: "{"),
              let timestampText = value(after: timestampKey, in: line, backwards: true),
              let timestamp = ISO8601Fast.parse(timestampText)
        else { return nil }
        let head = UnsafeRawBufferPointer(rebasing: line[0..<min(prefixLength, line.count)])
        let role: TranscriptEvent.Role
        // The message role precedes the content in every format; the assistant marker is checked first because
        // a tool call's input may legitimately contain a `role` argument.
        if find(assistantRoleMarker, in: head) != nil || find(assistantMarker, in: head) != nil {
            role = .assistant
        } else if find(userRoleMarker, in: head) != nil || find(userMarker, in: head) != nil {
            role = .user
        } else {
            role = .other
        }
        var input = 0, cacheCreation = 0, cacheRead = 0, output = 0
        var model: String?
        var messageId: String?
        var requestId: String?
        var stopReason: String?
        if role == .assistant {
            if let usage = usageObject(in: line) {
                input = count(usage["input_tokens"])
                cacheCreation = count(usage["cache_creation_input_tokens"])
                cacheRead = count(usage["cache_read_input_tokens"])
                output = count(usage["output_tokens"])
            }
            model = value(after: modelKey, in: head)
            messageId = value(after: messageIdKey, in: head) ?? value(after: fallbackIdKey, in: head).map { "msg_" + $0 }
            requestId = value(after: requestIdKey, in: line, backwards: true)
            // The message-level reason follows the content, so the last occurrence is the real one.
            stopReason = stringValue(after: stopReasonKey, in: line)
        }
        var text: String?
        var isPrompt = false
        if role == .user, find(toolResultMarker, in: head) == nil {
            text = value(after: contentStringKey, in: line, keyIn: head) ?? value(after: contentTextKey, in: line, keyIn: head)
            // Command output, attachments and compaction summaries are user lines flagged after their content.
            isPrompt = find(metaMarker, in: line, backwards: true) == nil
        }
        let tailStart = max(0, line.count - tailLength)
        let tail = UnsafeRawBufferPointer(rebasing: line[tailStart...])
        let entrypoint = find(entrypointKey, in: tail, backwards: true).flatMap { string(from: tailStart + $0 + entrypointKey.count, in: line) }
            ?? value(after: entrypointKey, in: line, keyIn: head)
        return TranscriptEvent(
            timestamp: timestamp,
            role: role,
            model: model,
            inputTokens: input,
            cacheCreationTokens: cacheCreation,
            cacheReadTokens: cacheRead,
            outputTokens: output,
            text: text,
            sessionId: value(after: sessionIdKey, in: head),
            cwd: value(after: cwdKey, in: head),
            messageId: messageId,
            requestId: requestId,
            stopReason: stopReason,
            isSidechain: find(sidechainMarker, in: head) != nil,
            isPrompt: isPrompt,
            entrypoint: entrypoint
        )
    }

    // MARK: Searching

    /// Offset of `pattern` in `buffer`, forward via memmem or backwards by scanning from the end.
    private static func find(_ pattern: [UInt8], in buffer: UnsafeRawBufferPointer, backwards: Bool = false) -> Int? {
        guard let base = buffer.baseAddress, buffer.count >= pattern.count, !pattern.isEmpty else { return nil }
        if !backwards {
            return pattern.withUnsafeBytes { needle -> Int? in
                guard let hit = memmem(base, buffer.count, needle.baseAddress, needle.count) else { return nil }
                return base.distance(to: hit)
            }
        }
        let first = pattern[0]
        var index = buffer.count - pattern.count
        while index >= 0 {
            if buffer[index] == first {
                var matched = true
                for offset in 1..<pattern.count where buffer[index + offset] != pattern[offset] {
                    matched = false
                    break
                }
                if matched { return index }
            }
            index -= 1
        }
        return nil
    }

    /// The JSON string value that follows `key` (which ends with the opening quote), unescaped.
    /// - keyIn: where to look for the key (defaults to `line`); the value is read from `line` from that offset.
    private static func value(after key: [UInt8], in line: UnsafeRawBufferPointer, backwards: Bool = false, keyIn: UnsafeRawBufferPointer? = nil) -> String? {
        let haystack = keyIn ?? line
        guard let keyOffset = find(key, in: haystack, backwards: backwards) else { return nil }
        return string(from: keyOffset + key.count, in: line)
    }

    /// The JSON string after `key` (which ends with the colon), searched backwards; nil when the value is null.
    private static func stringValue(after key: [UInt8], in line: UnsafeRawBufferPointer) -> String? {
        guard let keyOffset = find(key, in: line, backwards: true) else { return nil }
        let start = keyOffset + key.count
        guard start < line.count, line[start] == UInt8(ascii: "\"") else { return nil }
        return string(from: start + 1, in: line)
    }

    /// The JSON string whose first byte after the opening quote is at `start`, unescaped.
    private static func string(from start: Int, in line: UnsafeRawBufferPointer) -> String? {
        var index = start
        var escaped = false
        var sawBackslash = false
        while index < line.count {
            let byte = line[index]
            if escaped {
                escaped = false
            } else if byte == UInt8(ascii: "\\") {
                escaped = true
                sawBackslash = true
            } else if byte == UInt8(ascii: "\"") {
                break
            }
            index += 1
        }
        guard index < line.count else { return nil }
        let raw = UnsafeRawBufferPointer(rebasing: line[start..<index])
        if !sawBackslash {
            return String(decoding: raw, as: UTF8.self)
        }
        // Let JSONSerialization handle escapes such as \n, \" and \uXXXX.
        var wrapped = Data("[\"".utf8)
        wrapped.append(contentsOf: raw)
        wrapped.append(Data("\"]".utf8))
        return (try? JSONSerialization.jsonObject(with: wrapped) as? [String])?.first
    }

    /// The `usage` object as a dictionary (brace-matched slice fed to JSONSerialization). The real usage object is
    /// the last one on the line, after the message content.
    private static func usageObject(in line: UnsafeRawBufferPointer) -> [String: Any]? {
        guard let keyOffset = find(usageKey, in: line, backwards: true) else { return nil }
        let open = keyOffset + usageKey.count - 1
        var depth = 0
        var index = open
        var inString = false
        var escaped = false
        while index < line.count {
            let byte = line[index]
            if inString {
                if escaped {
                    escaped = false
                } else if byte == UInt8(ascii: "\\") {
                    escaped = true
                } else if byte == UInt8(ascii: "\"") {
                    inString = false
                }
            } else if byte == UInt8(ascii: "\"") {
                inString = true
            } else if byte == UInt8(ascii: "{") {
                depth += 1
            } else if byte == UInt8(ascii: "}") {
                depth -= 1
                if depth == 0 {
                    let slice = Data(UnsafeRawBufferPointer(rebasing: line[open...index]))
                    return try? JSONSerialization.jsonObject(with: slice) as? [String: Any]
                }
            }
            index += 1
        }
        return nil
    }

    private static func count(_ value: Any?) -> Int {
        if let int = value as? Int { return int }
        if let double = value as? Double { return Int(double) }
        return 0
    }
}

/// Hand-rolled parser for the fixed `YYYY-MM-DDTHH:MM:SS[.fff…](Z|±HH:MM)` shape Claude Code writes;
/// about two orders of magnitude cheaper than `ISO8601FormatStyle`. Anything else falls back to `DateParsing`.
public enum ISO8601Fast {
    public static func parse(_ text: String) -> Date? {
        let bytes = Array(text.utf8)
        guard bytes.count >= 20,
              let year = digits(bytes, 0, 4), bytes[4] == UInt8(ascii: "-"),
              let month = digits(bytes, 5, 2), bytes[7] == UInt8(ascii: "-"),
              let day = digits(bytes, 8, 2), bytes[10] == UInt8(ascii: "T"),
              let hour = digits(bytes, 11, 2), bytes[13] == UInt8(ascii: ":"),
              let minute = digits(bytes, 14, 2), bytes[16] == UInt8(ascii: ":"),
              let second = digits(bytes, 17, 2)
        else { return DateParsing.iso8601(text) }
        var index = 19
        var fraction = 0.0
        if index < bytes.count, bytes[index] == UInt8(ascii: ".") {
            index += 1
            var scale = 0.1
            while index < bytes.count, bytes[index] >= 48, bytes[index] <= 57 {
                fraction += Double(bytes[index] - 48) * scale
                scale /= 10
                index += 1
            }
        }
        var offsetSeconds = 0
        guard index < bytes.count else { return DateParsing.iso8601(text) }
        switch bytes[index] {
        case UInt8(ascii: "Z"):
            break
        case UInt8(ascii: "+"), UInt8(ascii: "-"):
            guard let offsetHour = digits(bytes, index + 1, 2) else { return DateParsing.iso8601(text) }
            let offsetMinute = (index + 5 < bytes.count) ? (digits(bytes, index + 4, 2) ?? 0) : 0
            offsetSeconds = (offsetHour * 3600 + offsetMinute * 60) * (bytes[index] == UInt8(ascii: "+") ? 1 : -1)
        default:
            return DateParsing.iso8601(text)
        }
        guard (1...12).contains(month), (1...31).contains(day), hour < 24, minute < 60, second < 61 else { return nil }
        let days = daysFromCivil(year: year, month: month, day: day)
        let seconds = Double(days * 86400 + hour * 3600 + minute * 60 + second - offsetSeconds) + fraction
        return Date(timeIntervalSince1970: seconds)
    }

    private static func digits(_ bytes: [UInt8], _ start: Int, _ count: Int) -> Int? {
        guard start + count <= bytes.count else { return nil }
        var value = 0
        for index in start..<(start + count) {
            let byte = bytes[index]
            guard byte >= 48, byte <= 57 else { return nil }
            value = value * 10 + Int(byte - 48)
        }
        return value
    }

    /// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
    static func daysFromCivil(year: Int, month: Int, day: Int) -> Int {
        let y = month <= 2 ? year - 1 : year
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }
}
