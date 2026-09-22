import AgentHUDSupport
import Foundation

/// How a Claude Code session record shows that a call its hook is waiting on has been settled elsewhere.
///
/// Claude Code keeps the hook running after its user answers in Claude Code's own dialog, and tells it nothing: the
/// only trace of that answer is the session record, where the call and then its result are written. The record names
/// calls by an id the hook is never given, so a call is recognized by its tool and its exact input. A question is
/// written only once it has been answered, together with its answers; a tool call is written before it is asked about
/// and its result after it has run.
struct PermissionTranscript {
    let tool: String
    let input: JSONValue
    /// Calls in the record with this tool and input whose result has not been written yet.
    private(set) var calls: Set<String> = []

    private static let useMarker = Data(#""type":"tool_use""#.utf8)
    private static let resultMarker = Data(#""type":"tool_result""#.utf8)
    private let nameMarker: Data

    init(tool: String, input: JSONValue) {
        self.tool = tool
        self.input = input
        nameMarker = Data("\"name\":\"\(tool)\"".utf8)
    }

    /// Reads one line of the record, and says whether it settled one of these calls. Most lines are passed over
    /// without being decoded: only a call to this tool, or a result while one of its calls is open, is worth it.
    mutating func read(_ line: Data) -> Bool {
        let use = line.range(of: Self.useMarker) != nil && line.range(of: nameMarker) != nil
        let result = !calls.isEmpty && line.range(of: Self.resultMarker) != nil
            && calls.contains { line.range(of: Data($0.utf8)) != nil }
        guard use || result, let blocks = try? ProviderJSON.read(line)["message"]["content"].arrayValue else { return false }
        var settled = false
        for block in blocks {
            switch block["type"].stringValue {
            case "tool_use":
                if block["name"].stringValue == tool, block["input"] == input, let id = block["id"].stringValue {
                    calls.insert(id)
                }
            case "tool_result":
                if let id = block["tool_use_id"].stringValue, calls.remove(id) != nil { settled = true }
            default:
                break
            }
        }
        return settled
    }
}

/// Follows one session record from the hook until the call it asked about is settled, then says so once.
///
/// What is already in the record is read first, so a call written before the hook started is known and an earlier
/// call with the same input that already has its result is not mistaken for this one. After that only what the client
/// appends is read, as it is written.
final class PermissionTranscriptWatch: @unchecked Sendable {
    /// How far back the record is read for a call written before the hook started. The call is among the last lines;
    /// this only has to hold a large one.
    static let history: UInt64 = 4 * 1024 * 1024

    private let queue = DispatchQueue(label: "app.agenthud.permission.transcript")
    private let handle: FileHandle
    private let source: DispatchSourceFileSystemObject
    private var transcript: PermissionTranscript
    private var partial = Data()
    private var done = false
    private let settled: @Sendable () -> Void

    /// Nothing is watched without a record to read, or for a payload that does not name its call.
    init?(payload: JSONValue, settled: @escaping @Sendable () -> Void) {
        guard let path = payload["transcript_path"].stringValue, !path.isEmpty,
              let tool = payload["tool_name"].stringValue, !tool.isEmpty,
              let handle = FileHandle(forReadingAtPath: path) else { return nil }
        self.handle = handle
        self.settled = settled
        transcript = PermissionTranscript(tool: tool, input: payload["tool_input"])
        source = DispatchSource.makeFileSystemObjectSource(fileDescriptor: handle.fileDescriptor,
                                                           eventMask: [.extend, .write], queue: queue)

        let end = handle.seekToEndOfFile()
        let start = end > Self.history ? end - Self.history : 0
        handle.seek(toFileOffset: start)
        var earlier = handle.readData(ofLength: Int(end - start))
        // A read that starts inside a line cannot parse it; the line it cut is older than any call worth finding.
        if start > 0 { earlier = earlier.firstIndex(of: 0x0A).map { earlier.subdata(in: earlier.index(after: $0)..<earlier.endIndex) } ?? Data() }
        // Results already in the record belong to earlier calls, so they settle nothing here.
        _ = lines(in: earlier)

        source.setEventHandler { [weak self] in self?.follow() }
        source.resume()
        // Whatever was appended between the read above and the source starting would otherwise wait for the next write.
        queue.async { [weak self] in self?.follow() }
    }

    deinit { source.cancel() }

    /// Stops following, so nothing is said after this returns.
    func stop() {
        queue.sync {
            done = true
            source.cancel()
        }
    }

    private func follow() {
        guard !done, lines(in: handle.availableData) else { return }
        done = true
        source.cancel()
        settled()
    }

    /// Feeds the complete lines in `data` to the reader and keeps a trailing partial line for the next read.
    /// Returns whether any of them settled the call.
    private func lines(in data: Data) -> Bool {
        partial.append(data)
        var settledNow = false
        var start = partial.startIndex
        while let newline = partial[start...].firstIndex(of: 0x0A) {
            if newline > start, transcript.read(partial[start..<newline]) { settledNow = true }
            start = partial.index(after: newline)
        }
        partial = partial.subdata(in: start..<partial.endIndex)
        return settledNow
    }
}
