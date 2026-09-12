import AgentHUDSupport
import Foundation

/// Pi's native lifecycle events are independent of its persisted message usage.
public enum PiSessionObserver {
    private static let filename = "agent-hud.ts"

    /// Adapter setup is independent of the user's live-status presentation preference.
    public static func configureIfAvailable(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                            environment: [String: String] = ProcessInfo.processInfo.environment) throws {
        guard FileManager.default.fileExists(atPath: OpenAgentPaths(home: home, environment: environment).pi.path) else { return }
        try configure(enabled: true, home: home, environment: environment)
    }

    public static func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                   environment: [String: String] = ProcessInfo.processInfo.environment) -> Bool {
        let paths = OpenAgentPaths(home: home, environment: environment)
        return (try? String(contentsOf: paths.pi.appendingPathComponent("extensions/\(filename)"), encoding: .utf8)) == script
    }

    public static func configure(enabled: Bool, home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                 environment: [String: String] = ProcessInfo.processInfo.environment) throws {
        let paths = OpenAgentPaths(home: home, environment: environment)
        let file = paths.pi.appendingPathComponent("extensions/\(filename)")
        let previous = try? String(contentsOf: file, encoding: .utf8)
        // Only replace/remove our own extension, never an unrelated file with the same name.
        guard previous == nil || previous!.hasPrefix("// Agent HUD Pi session observer\n") else {
            throw UsageProviderError(L10n.text("agent-hud.ts 已被其他扩展使用", "agent-hud.ts belongs to another extension"))
        }
        if enabled {
            guard previous != script else { return }
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(script.utf8).write(to: file, options: .atomic)
        } else if previous != nil {
            try FileManager.default.removeItem(at: file)
        }
    }

    struct Observation: Codable, Sendable {
        let version: Int
        let sessionID: String
        let sessionFile: String?
        let workspace: String
        let title: String
        let model: String?
        let providerID: String?
        let turnID: String
        let state: SessionTurn.State
        let startedAtMs: Int64
        let observedAtMs: Int64

        var turn: SessionTurn {
            .init(provider: "Pi", sessionID: sessionID, turnID: turnID, state: state,
                  startedAtMs: startedAtMs, observedAtMs: observedAtMs)
        }

        var session: OpenAgentSession {
            var value = OpenAgentSession(id: sessionID, client: .pi, title: title, workspace: workspace,
                path: sessionFile ?? "", start: RecordCoding.date(startedAtMs), end: RecordCoding.date(observedAtMs), turns: [turn])
            if let model, let providerID { value.setModel(model, provider: providerID) }
            if state == .completed {
                value.completions = [.init(sessionID: sessionID, vendor: "Pi", turnID: turnID,
                    task: title, model: model ?? "Unknown", startedAt: RecordCoding.date(startedAtMs), completedAt: RecordCoding.date(observedAtMs))]
            }
            return value
        }
    }

    static func read(_ data: Data) throws -> Observation {
        guard data.count <= 64 * 1024 else { throw ProviderFailure.limit }
        let value = try JSONDecoder().decode(Observation.self, from: data)
        guard value.version == 1, value.sessionID.hasPrefix("pi:"), value.sessionID.count > 3,
              !value.turnID.isEmpty, value.startedAtMs > 0, value.observedAtMs >= value.startedAtMs else {
            throw ProviderFailure.format
        }
        return value
    }

    // No Pi imports are required: this works with Pi's built-in extension loader.
    // Only lifecycle metadata leaves the process. Tokens remain owned by Pi's transcript.
    static let script = #"""
    // Agent HUD Pi session observer
    import { mkdirSync, writeFileSync, renameSync, readdirSync, statSync, unlinkSync } from "node:fs";
    import { homedir } from "node:os";
    import { join } from "node:path";
    import { createHash, randomUUID } from "node:crypto";

    export default function (pi) {
      const directory = join(process.env.PI_CODING_AGENT_DIR || join(homedir(), ".pi", "agent"), "agent-hud", "turns");
      let active;
      let heartbeat;
      let lastStopReason;

      function publish(ctx, state = "running") {
        if (!active) return;
        const sessionID = "pi:" + ctx.sessionManager.getSessionId();
        const record = {
          version: 1, sessionID, sessionFile: ctx.sessionManager.getSessionFile(),
          workspace: ctx.cwd, title: ctx.sessionManager.getSessionName() || "Pi",
          model: ctx.model?.id, providerID: ctx.model?.provider,
          turnID: active.id, state, startedAtMs: active.startedAtMs, observedAtMs: Date.now(),
        };
        try {
          mkdirSync(directory, { recursive: true, mode: 0o700 });
          const name = createHash("sha256").update(sessionID + "\0" + active.id).digest("hex");
          const file = join(directory, name + ".json");
          const temporary = file + "." + process.pid + ".tmp";
          writeFileSync(temporary, JSON.stringify(record), { mode: 0o600 });
          renameSync(temporary, file);
        } catch { /* Observability must never interrupt Pi's agent loop. */ }
      }

      function finish(ctx, state) {
        publish(ctx, state);
        clearInterval(heartbeat);
        heartbeat = undefined;
        active = undefined;
      }

      pi.on("session_start", () => {
        try {
          for (const name of readdirSync(directory)) {
            if (/^[a-f0-9]{64}\.json$/.test(name) && statSync(join(directory, name)).mtimeMs < Date.now() - 7 * 86400000) {
              unlinkSync(join(directory, name));
            }
          }
        } catch { /* The inbox is created on the first agent run. */ }
      });
      pi.on("agent_start", (_event, ctx) => {
        // Retries, auto-compaction and queued follow-ups belong to one unsettled run.
        if (!active) {
          active = { id: randomUUID(), startedAtMs: Date.now() };
          heartbeat = setInterval(() => publish(ctx), 15000);
          heartbeat.unref();
        }
        lastStopReason = undefined;
        publish(ctx);
      });
      pi.on("message_end", (event, ctx) => {
        if (event.message.role === "assistant") lastStopReason = event.message.stopReason;
        publish(ctx);
      });
      pi.on("tool_execution_start", (_event, ctx) => publish(ctx));
      pi.on("tool_execution_end", (_event, ctx) => publish(ctx));
      pi.on("agent_settled", (_event, ctx) => finish(ctx, lastStopReason === "stop" ? "completed" : "ended"));
      pi.on("session_shutdown", (_event, ctx) => finish(ctx, "ended"));
    }
    """#
}
