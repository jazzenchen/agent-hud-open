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
    /// How much of an assistant message is kept. Readers truncate further; this only bounds the work of reading it.
    static let messageLength = 2048
    /// Current builds write `entrypoint`, `cwd`, `sessionId` and `version` after the message, close to the end.
    private static let tailLength = 2048

    private static let timestampKey = Array("\"timestamp\":\"".utf8)
    private static let assistantRoleMarker = Array("\"role\":\"assistant\"".utf8)
    private static let userRoleMarker = Array("\"role\":\"user\"".utf8)
    private static let assistantMarker = Array("\"type\":\"assistant\"".utf8)
    private static let userMarker = Array("\"type\":\"user\"".utf8)
    private static let sidechainMarker = Array("\"isSidechain\":true".utf8)
    private static let metaMarker = Array("\"isMeta\":true".utf8)
    private static let summaryMarker = Array("\"isCompactSummary\":true".utf8)
    private static let compactionMarker = Array("\"subtype\":\"compact_boundary\"".utf8)
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
        var input = 0, cacheCreation = 0, cacheRead = 0, output = 0, thinking = 0
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
                thinking = count((usage["output_tokens_details"] as? [String: Any])?["thinking_tokens"])
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
            isPrompt = find(metaMarker, in: line, backwards: true) == nil && find(summaryMarker, in: line, backwards: true) == nil
        } else if role == .assistant {
            // A visible answer. A thinking or tool-use block is a different type and never matches this key.
            text = value(after: contentTextKey, in: line, keyIn: head, limit: messageLength)
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
            thinkingTokens: thinking,
            text: text,
            sessionId: value(after: sessionIdKey, in: head),
            cwd: value(after: cwdKey, in: head),
            messageId: messageId,
            requestId: requestId,
            stopReason: stopReason,
            isSidechain: find(sidechainMarker, in: head) != nil,
            isPrompt: isPrompt,
            isCompaction: role == .other && find(compactionMarker, in: head) != nil,
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
    private static func value(after key: [UInt8], in line: UnsafeRawBufferPointer, backwards: Bool = false,
                              keyIn: UnsafeRawBufferPointer? = nil, limit: Int? = nil) -> String? {
        let haystack = keyIn ?? line
        guard let keyOffset = find(key, in: haystack, backwards: backwards) else { return nil }
        return string(from: keyOffset + key.count, in: line, limit: limit)
    }

    /// The JSON string after `key` (which ends with the colon), searched backwards; nil when the value is null.
    private static func stringValue(after key: [UInt8], in line: UnsafeRawBufferPointer) -> String? {
        guard let keyOffset = find(key, in: line, backwards: true) else { return nil }
        let start = keyOffset + key.count
        guard start < line.count, line[start] == UInt8(ascii: "\"") else { return nil }
        return string(from: start + 1, in: line)
    }

    /// The JSON string whose first byte after the opening quote is at `start`, unescaped.
    /// - limit: stop after this many bytes and return what was read, cut where an escape or a UTF-8 sequence allows;
    ///   without it the whole value is read, however long it is.
    private static func string(from start: Int, in line: UnsafeRawBufferPointer, limit: Int? = nil) -> String? {
        let stop = limit.map { min(line.count, start + $0) } ?? line.count
        var index = start
        // Bytes still belonging to an escape sequence; a value can only be cut where none are outstanding.
        var pending = 0
        var sawBackslash = false
        var cut = start
        while index < stop {
            let byte = line[index]
            if pending == 0, byte & 0xC0 != 0x80 { cut = index }
            if pending > 0 {
                pending -= 1
            } else if byte == UInt8(ascii: "\\") {
                sawBackslash = true
                // \uXXXX carries four hex digits after the u; every other escape is one byte.
                pending = index + 1 < stop && line[index + 1] == UInt8(ascii: "u") ? 5 : 1
            } else if byte == UInt8(ascii: "\"") {
                break
            }
            index += 1
        }
        if index >= stop, limit != nil, stop < line.count {
            guard cut > start else { return nil }
            return decode(UnsafeRawBufferPointer(rebasing: line[start..<cut]), escaped: sawBackslash)
        }
        guard index < line.count else { return nil }
        let raw = UnsafeRawBufferPointer(rebasing: line[start..<index])
        return decode(raw, escaped: sawBackslash)
    }

    private static func decode(_ raw: UnsafeRawBufferPointer, escaped: Bool) -> String? {
        if !escaped { return String(decoding: raw, as: UTF8.self) }
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
