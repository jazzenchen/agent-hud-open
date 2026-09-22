import AgentHUDSupport
import Darwin
import Foundation

/// The `--permission-hook` side: a short-lived process the client starts and then waits on.
///
/// It hands the client's request to the running HUD and prints back whatever the user decided. Every failure — no HUD,
/// a wedged HUD, a payload it cannot read — prints nothing, which leaves the client's own permission flow exactly as
/// it would be with no hook installed. The wait itself is not bounded here: the client already set the limit it is
/// willing to wait, and the HUD learns the request is over when this process goes away — which it also does on its
/// own once the session record shows the user answered in the client instead.
public enum PermissionHookClient {
    /// The field the hook adds so the HUD knows which client is asking; the client's own fields are left untouched.
    public static let sourceKey = "_agentHUDSource"

    public static func run(source: PermissionHooks.Source) {
        // A HUD that stops reading must never raise a signal in the client's hook process.
        signal(SIGPIPE, SIG_IGN)
        signal(SIGALRM) { _ in _exit(0) }

        // Everything up to the wait is bounded: a client that forgot to close its pipe, or a socket that will not
        // accept, must not hold the client here.
        alarm(5)
        let input = FileHandle.standardInput.readDataToEndOfFile()
        guard let payload = tagged(input, source: source) else { return }
        guard let socket = connect(to: PermissionRequests.socketPath) else { return }
        defer { close(socket) }
        guard send(socket, payload) else { return }
        // Half-close so the HUD sees the whole request and knows no more is coming.
        shutdown(socket, SHUT_WR)
        alarm(0)

        // Answered in the client's own dialog, the call shows up settled in the session record while this hook is
        // still waiting; leaving without an answer then takes the request off the HUD.
        let watch = source.recordsCalls
            ? (try? ProviderJSON.read(input)).flatMap { PermissionTranscriptWatch(payload: $0) { _exit(0) } }
            : nil
        let response = readToEnd(socket)
        // The HUD answered first; the answer is written whole.
        watch?.stop()
        guard !response.isEmpty else { return }
        FileHandle.standardOutput.write(response)
    }

    /// Adds the client's name to the payload. A payload that is not a JSON object is not one of ours to forward.
    static func tagged(_ data: Data, source: PermissionHooks.Source) -> Data? {
        guard !data.isEmpty, data.count <= 1024 * 1024,
              let json = try? ProviderJSON.read(data), var object = json.objectValue else { return nil }
        object[sourceKey] = .string(source.rawValue)
        return try? RecordCoding.encoder().encode(ProviderJSON.object(object))
    }

    private static func connect(to path: String) -> Int32? {
        var info = stat()
        guard path.utf8.count < 104, stat(path, &info) == 0, (info.st_mode & S_IFMT) == S_IFSOCK else { return nil }
        let descriptor = socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return nil }
        var on: Int32 = 1
        setsockopt(descriptor, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let size = MemoryLayout.size(ofValue: address.sun_path)
        _ = withUnsafeMutablePointer(to: &address.sun_path) { field in
            field.withMemoryRebound(to: CChar.self, capacity: size) { strlcpy($0, path, size) }
        }
        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else {
            close(descriptor)
            return nil
        }
        return descriptor
    }

    private static func send(_ descriptor: Int32, _ data: Data) -> Bool {
        data.withUnsafeBytes { buffer in
            guard let base = buffer.baseAddress else { return false }
            var sent = 0
            while sent < buffer.count {
                let written = Darwin.send(descriptor, base + sent, buffer.count - sent, 0)
                if written < 0 {
                    if errno == EINTR { continue }
                    return false
                }
                if written == 0 { return false }
                sent += written
            }
            return true
        }
    }

    private static func readToEnd(_ descriptor: Int32) -> Data {
        var response = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while response.count <= 1024 * 1024 {
            let read = recv(descriptor, &buffer, buffer.count, 0)
            if read < 0 {
                if errno == EINTR { continue }
                break
            }
            if read == 0 { break }
            response.append(contentsOf: buffer[..<read])
        }
        return response
    }
}
