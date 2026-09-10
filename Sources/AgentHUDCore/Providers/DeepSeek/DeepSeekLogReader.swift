import Foundation

public enum DeepSeekLocator {
    public static var dataDirectory: URL { dataDirectory(environment: ProcessInfo.processInfo.environment) }

    public static func dataDirectory(environment: [String: String], home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        guard let path = environment["DSH_HOME"], !path.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return home.appendingPathComponent(".dsh", isDirectory: true)
        }
        let expanded = path == "~" ? home.path : path.hasPrefix("~/") ? home.appendingPathComponent(String(path.dropFirst(2))).path : path
        return URL(fileURLWithPath: expanded, isDirectory: true).standardizedFileURL
    }

    public static func isInstalled(directory: URL = dataDirectory) -> Bool {
        ["profiles", "sessions"].contains { name in
            var isDirectory: ObjCBool = false
            return FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path, isDirectory: &isDirectory) && isDirectory.boolValue
        }
    }

    /// Harness itself requires Node with Zstandard support. GUI launches may have a minimal PATH.
    public static func nodeExecutable(path: String = ProcessInfo.processInfo.environment["PATH"] ?? "") -> URL? {
        let candidates = path.split(separator: ":").map { URL(fileURLWithPath: String($0)).appendingPathComponent("node") }
            + ["/opt/homebrew/bin/node", "/usr/local/bin/node"].map { URL(fileURLWithPath: $0) }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }


}

enum DeepSeekLogReader {
    static func read(_ url: URL) throws -> Data {
        guard url.pathExtension == "zstd" else { return try Data(contentsOf: url, options: .mappedIfSafe) }
        // Node's decoder stops at one frame. Harness appends a frame per write batch.
        return try DeepSeekNode.run(script: """
        const {readFileSync} = require('node:fs');
        const {zstdDecompressSync, constants} = require('node:zlib');
        const input = readFileSync(process.argv[1]);
        for (let offset = 0; offset < input.length;) {
          if (input.length - offset < 4) break;
          const {buffer, engine} = zstdDecompressSync(input.subarray(offset),
            {info: true, finishFlush: constants.ZSTD_e_flush});
          if (!engine.bytesWritten) throw new Error('Invalid Zstandard frame');
          process.stdout.write(buffer);
          offset += engine.bytesWritten;
        }
        """, arguments: [url.path])
    }
}

public enum DeepSeekNode {
    public static func run(script: String, arguments: [String], environment: [String: String] = [:]) throws -> Data {
        guard let node = DeepSeekLocator.nodeExecutable() else {
            throw UsageProviderError(L10n.text("读取 Harness 数据需要 Node.js", "Node.js is required to read Harness data"))
        }
        let process = Process(), output = Pipe()
        process.executableURL = node
        process.arguments = ["-e", script] + arguments
        process.environment = environment
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        try process.run()
        // Drain before waiting: a session can be much larger than the pipe buffer.
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw UsageProviderError(L10n.text("Harness 数据读取失败，请检查本机安装和 Node.js 版本", "Cannot read Harness data; check the local install and Node.js version"))
        }
        return data
    }
}
