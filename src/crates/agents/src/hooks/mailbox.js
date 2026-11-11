// Agent HUD's mailbox. Other agents' messages for a session wait in Agent HUD's hook channel (CHANNEL, above, written in
// when Agent HUD installs this file; null leaves the mailbox out). The observer hands each to its session as the next
// prompt, oldest first, claiming it the way Agent HUD's own channel does (`ClaudeChannel::take_next`): an exclusive
// `<id>.claim` beside the message, then `takenAt` and `startedAt` in it. While the process runs it says which sessions it
// can take messages for (`observers/`), and it notes each call a session makes to Agent HUD's own MCP tools (`calls/`),
// which is how Agent HUD knows the sender of a message. Nothing here may ever throw into the client.
import * as agentHudFs from "node:fs";
import { join as agentHudJoin } from "node:path";
import { createHash as agentHudHash } from "node:crypto";

function agentHudMailbox(source) {
  const valid = (id) => typeof id === "string" && id.length > 0 && id.length <= 116 && /^[A-Za-z0-9_-]+$/.test(id);
  // A client may load the plugin more than once in one process (OpenCode, once per folder it serves).
  const instance = process.pid + "-" + agentHudHash("sha256").update(String(Math.random()) + Date.now()).digest("hex").slice(0, 12);
  const presence = () => agentHudJoin(CHANNEL, "observers", source + "@" + instance + ".json");
  const calls = () => agentHudJoin(CHANNEL, "calls", String(process.pid));
  function write(file, value) {
    const temporary = file + "." + process.pid + ".tmp";
    agentHudFs.writeFileSync(temporary, JSON.stringify(value), { mode: 0o600 });
    agentHudFs.renameSync(temporary, file);
  }
  function folder(path) {
    agentHudFs.mkdirSync(path, { recursive: true, mode: 0o700 });
    return path;
  }

  return {
    // The sessions this process can take messages for now, newest first.
    present(ids) {
      if (!CHANNEL) return;
      try {
        const observers = folder(agentHudJoin(CHANNEL, "observers"));
        write(presence(), { pid: process.pid, at: Date.now(), client: source, sessions: ids.filter(valid).slice(0, 200) });
        // Words left by processes that ended without saying so (killed, crashed) go.
        for (const name of agentHudFs.readdirSync(observers)) {
          const pid = Number(/@(\d+)-[0-9a-f]+\.json$/.exec(name)?.[1]);
          if (!(pid > 1) || pid === process.pid) continue;
          try { process.kill(pid, 0); } catch (error) {
            if (error?.code === "ESRCH") try { agentHudFs.unlinkSync(agentHudJoin(observers, name)); } catch { /* Gone. */ }
          }
        }
      } catch { /* Agent HUD then says the session takes a message only when it is open again. */ }
    },
    absent() {
      if (!CHANNEL) return;
      try { agentHudFs.unlinkSync(presence()); } catch { /* Already gone. */ }
    },
    // The sessions of this client that have messages waiting, by id.
    boxes() {
      if (!CHANNEL) return [];
      try {
        return agentHudFs.readdirSync(agentHudJoin(CHANNEL, "prompts"))
          .filter((name) => name.startsWith(source + "--"))
          .map((name) => name.slice(source.length + 2))
          .filter(valid);
      } catch { return []; }
    },
    // The oldest message waiting for session `id`, now claimed by this process: `{ id, text }`, else undefined.
    take(id) {
      if (!CHANNEL || !valid(id)) return undefined;
      const box = agentHudJoin(CHANNEL, "prompts", source + "--" + id);
      let names;
      try { names = agentHudFs.readdirSync(box).filter((name) => /^[A-Za-z0-9_-]+\.json$/.test(name)); } catch { return undefined; }
      const waiting = [];
      for (const name of names) {
        try {
          const value = JSON.parse(agentHudFs.readFileSync(agentHudJoin(box, name), "utf8"));
          if (typeof value.text !== "string") continue;
          if (["takenAt", "startedAt", "cancelledAt", "expiredAt", "refused"].some((key) => value[key] !== undefined)) continue;
          waiting.push({ path: agentHudJoin(box, name), id: name.slice(0, -5), value });
        } catch { /* A message still being written is read on a later look. */ }
      }
      waiting.sort((a, b) => (Number(a.value.at) || 0) - (Number(b.value.at) || 0) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      for (const { path, id: message, value } of waiting) {
        const at = Date.now(), base = path.slice(0, -".json".length);
        try {
          if (agentHudFs.existsSync(base + ".cancel")) {
            write(path, { ...value, cancelledAt: at });
            try { agentHudFs.unlinkSync(base + ".cancel"); } catch { /* Already gone. */ }
            continue;
          }
          if (typeof value.expiresAt === "number" && value.expiresAt <= at) {
            write(path, { ...value, expiredAt: at });
            continue;
          }
          try {
            agentHudFs.closeSync(agentHudFs.openSync(base + ".claim", "wx", 0o600));
          } catch { continue; /* Another process took it a moment ago. */ }
          write(path, { ...value, takenAt: at, startedAt: at });
          return { id: message, text: value.text };
        } catch { /* The next look tries again. */ }
      }
      return undefined;
    },
    // The tool's name on Agent HUD's MCP server when `name` is one of them as the client calls it: `mcp__agent_hud__x`
    // in Pi, `agent-hud_x` in OpenCode. Undefined for any other tool.
    tool(name) {
      const match = /^(?:mcp__)?agent[-_]hud(?:__|_)([A-Za-z0-9_]+)$/.exec(typeof name === "string" ? name : "");
      return match ? match[1] : undefined;
    },
    // Session `id` is calling Agent HUD's tool `tool` with `args`: noted until `called(call)`.
    calling(id, tool, args, call) {
      if (!CHANNEL || !valid(id) || !tool) return;
      try {
        const name = agentHudHash("sha256").update(String(call ?? Math.random())).digest("hex");
        write(agentHudJoin(folder(calls()), name + ".json"), { tool, arguments: args ?? null, sessionId: source + ":" + id, at: Date.now() });
      } catch { /* Agent HUD then refuses to send without a known sender. */ }
    },
    called(call) {
      if (!CHANNEL) return;
      try {
        agentHudFs.unlinkSync(agentHudJoin(calls(), agentHudHash("sha256").update(String(call)).digest("hex") + ".json"));
      } catch { /* Already gone; notes also age out. */ }
    },
  };
}
