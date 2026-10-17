import AgentHUDSupport
import Foundation

/// OpenCode's stored messages carry no agent end, so a plugin Agent HUD keeps in OpenCode's global plugin directory
/// records each turn that ends in a final answer, and drops a session's records when OpenCode deletes it. It reports
/// completions only: usage still comes from OpenCode's store.
/// OpenCode 1.0 reads only `plugin/`, later releases `plugins/` as well, so the file goes in `plugin/`.
public enum OpenCodeSessionObserver {
    private static func file(_ paths: OpenAgentPaths) -> ObserverFile {
        .init(url: paths.openCodeConfig.appendingPathComponent("plugin/agent-hud.js"), marker: "// Agent HUD OpenCode session observer\n",
              script: script, conflict: L10n.text("agent-hud.js 已被其他插件使用", "agent-hud.js belongs to another plugin"))
    }

    /// OpenCode's data directory exists once OpenCode has run.
    static func isAvailable(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                            environment: [String: String] = ProcessInfo.processInfo.environment) -> Bool {
        FileManager.default.fileExists(atPath: OpenAgentPaths(home: home, environment: environment).openCode.path)
    }

    public static func configureIfAvailable(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                            environment: [String: String] = ProcessInfo.processInfo.environment) throws {
        guard isAvailable(home: home, environment: environment) else { return }
        try configure(enabled: true, home: home, environment: environment)
    }

    public static func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                   environment: [String: String] = ProcessInfo.processInfo.environment) -> Bool {
        file(OpenAgentPaths(home: home, environment: environment)).isCurrent
    }

    public static func configure(enabled: Bool, home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                 environment: [String: String] = ProcessInfo.processInfo.environment) throws {
        try file(OpenAgentPaths(home: home, environment: environment)).configure(enabled: enabled)
    }

    /// Whether the plugin is in place, for OpenCode's settings.
    public static func fileState(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                 environment: [String: String] = ProcessInfo.processInfo.environment) -> ClientObserverFile {
        file(OpenAgentPaths(home: home, environment: environment)).state
    }

    /// One completed turn of a top-level session: the user message that started it names the turn.
    struct Completion: Codable, Sendable {
        let version: Int
        let sessionID: String
        let workspace: String?
        let title: String?
        let model: String?
        let providerID: String?
        let turnID: String
        let startedAtMs: Int64
        let completedAtMs: Int64
        var navigationTarget: SessionNavigationTarget? = nil

        var session: OpenAgentSession {
            let named = OpenAgentParser.openCodeTitle(title)
            var value = OpenAgentSession(id: sessionID, client: .opencode,
                title: named ?? workspace.map { URL(fileURLWithPath: $0).lastPathComponent } ?? "OpenCode",
                titleSource: named == nil ? .placeholder : .observer, workspace: workspace, path: "",
                start: RecordCoding.date(startedAtMs), end: RecordCoding.date(completedAtMs))
            if let model, let providerID { value.setModel(model, provider: providerID) }
            value.navigation = .init(observedAt: RecordCoding.date(completedAtMs), target: navigationTarget)
            value.completions = [.init(sessionID: sessionID, vendor: "OpenCode", turnID: turnID, task: value.title,
                model: model ?? "Unknown", startedAt: RecordCoding.date(startedAtMs), completedAt: RecordCoding.date(completedAtMs),
                navigationTarget: navigationTarget)]
            return value
        }
    }

    static func read(_ data: Data) throws -> Completion {
        guard data.count <= 64 * 1024 else { throw ProviderFailure.limit }
        let value = try JSONDecoder().decode(Completion.self, from: data)
        guard value.version == 1, value.sessionID.hasPrefix("opencode:"), value.sessionID.count > 9,
              !value.turnID.isEmpty, value.startedAtMs > 0, value.completedAtMs >= value.startedAtMs else {
            throw ProviderFailure.format
        }
        return value
    }

    // No OpenCode imports: the plugin loads from a file OpenCode finds in its global plugin directory. Only metadata
    // leaves the process, and nothing it does can fail a turn.
    static let script = #"""
    // Agent HUD OpenCode session observer
    import { mkdirSync, writeFileSync, renameSync, readdirSync, readFileSync, statSync, unlinkSync } from "node:fs";
    import { homedir } from "node:os";
    import { join } from "node:path";
    import { createHash } from "node:crypto";

    export const AgentHUDSessionObserver = async (input) => {
      const directory = join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "opencode", "agent-hud", "turns");
      const sessions = new Map();
      try {
        for (const name of readdirSync(directory)) {
          if (/^[a-f0-9]{64}\.json$/.test(name) && statSync(join(directory, name)).mtimeMs < Date.now() - 7 * 86400000) {
            unlinkSync(join(directory, name));
          }
        }
      } catch { /* The inbox is created on the first completion. */ }

      function session(id) {
        let value = sessions.get(id);
        if (!value) sessions.set(id, value = {});
        return value;
      }

      // A session deleted in OpenCode takes its records along, so it leaves Agent HUD's list as it left OpenCode's.
      function forget(id) {
        try {
          for (const name of readdirSync(directory)) {
            if (!/^[a-f0-9]{64}\.json$/.test(name)) continue;
            try {
              if (JSON.parse(readFileSync(join(directory, name), "utf8")).sessionID === "opencode:" + id) unlinkSync(join(directory, name));
            } catch { /* A record that cannot be read is pruned with the week's. */ }
          }
        } catch { /* There are no records yet. */ }
      }

      function navigationTarget() {
        const id = process.env.ITERM_SESSION_ID;
        return process.env.TERM_PROGRAM === "iTerm.app" && !process.env.TMUX && id
          ? { kind: "iTermSession", id } : undefined;
      }

      // A busy stretch holds tool calls, retries, compaction and queued prompts, and ends when the session goes idle. It
      // completed when its newest reply answers its newest prompt, stopped on its own and carries no error. An error or an
      // abort makes the session idle before the reply that carries it is closed, so a reply still open is judged on closing.
      function settle(id, value) {
        const { reply, prompt, startedAtMs } = value;
        if (reply && !reply.completedAtMs && (!prompt || reply.parentID === prompt.id)) { value.waiting = true; return; }
        value.reply = value.prompt = value.startedAtMs = value.waiting = undefined;
        if (!reply || value.child || reply.error || reply.summary || reply.finish !== "stop") return;
        if (prompt && reply.parentID !== prompt.id) return;
        const record = {
          version: 1, sessionID: "opencode:" + id, workspace: value.directory || input.directory, title: value.title,
          model: reply.modelID, providerID: reply.providerID, turnID: reply.parentID,
          startedAtMs: startedAtMs || reply.createdAtMs || reply.completedAtMs, completedAtMs: Date.now(),
          navigationTarget: navigationTarget(),
        };
        try {
          mkdirSync(directory, { recursive: true, mode: 0o700 });
          const name = createHash("sha256").update(record.sessionID + "\0" + record.turnID).digest("hex");
          const file = join(directory, name + ".json");
          const temporary = file + "." + process.pid + ".tmp";
          writeFileSync(temporary, JSON.stringify(record), { mode: 0o600 });
          renameSync(temporary, file);
        } catch { /* Observability must never interrupt OpenCode. */ }
      }

      return {
        // Never throws or rejects: OpenCode 1.18 runs this inside the call that marks a session idle.
        event: async (payload) => {
          try {
            const event = payload?.event, properties = event?.properties || {}, info = properties.info || {};
            switch (event?.type) {
              case "session.created":
              case "session.updated":
                if (info.id) Object.assign(session(info.id), { child: !!info.parentID, title: info.title, directory: info.directory });
                break;
              case "session.deleted":
                if (info.id) { sessions.delete(info.id); forget(info.id); }
                break;
              case "message.updated": {
                // OpenCode updates earlier messages too, so the newest prompt and reply go by when they were created. The
                // message is live and keeps changing, so the fields are copied now.
                if (!info.sessionID || !info.id) break;
                const value = session(info.sessionID), createdAtMs = info.time?.created || 0;
                const newest = (known) => !known || known.id === info.id || createdAtMs >= known.createdAtMs;
                if (info.role === "user" && newest(value.prompt)) value.prompt = { id: info.id, createdAtMs };
                if (info.role === "assistant" && newest(value.reply)) {
                  value.reply = {
                    id: info.id, parentID: info.parentID, modelID: info.modelID, providerID: info.providerID, finish: info.finish,
                    error: !!info.error, summary: info.summary === true, createdAtMs, completedAtMs: info.time?.completed,
                  };
                  if (value.waiting && value.reply.completedAtMs) settle(info.sessionID, value);
                }
                break;
              }
              case "message.removed": {
                const value = sessions.get(properties.sessionID);
                if (value?.prompt?.id === properties.messageID) value.prompt = undefined;
                if (value?.reply?.id === properties.messageID) value.reply = undefined;
                break;
              }
              case "session.status":
              case "session.idle": {
                const value = sessions.get(properties.sessionID), type = event.type === "session.idle" ? "idle" : properties.status?.type;
                if (type === "busy" && properties.sessionID) {
                  const busy = session(properties.sessionID);
                  busy.startedAtMs ||= Date.now();
                  busy.waiting = undefined;
                }
                if (type === "idle" && value) settle(properties.sessionID, value);
                break;
              }
            }
          } catch { /* Observability must never interrupt OpenCode. */ }
        },
      };
    };
    """#
}
