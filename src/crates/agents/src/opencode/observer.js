// SOURCE and FOLDER, above, name the client: its sessions' id prefix and its data folder. What follows the script
// exports `server`, OpenCode 1's plugin, and for OpenCode itself also `setup`, OpenCode 2's (`export.js`).
import { mkdirSync, writeFileSync, renameSync, readdirSync, readFileSync, statSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";

// Each completed turn is one file, named by its session and its prompt, so a turn reported twice is written once.
// Records older than a week are pruned when the plugin loads.
function inbox() {
  const directory = join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), FOLDER, "agent-hud", "turns");
  try {
    for (const name of readdirSync(directory)) {
      if (/^[a-f0-9]{64}\.json$/.test(name) && statSync(join(directory, name)).mtimeMs < Date.now() - 7 * 86400000) {
        unlinkSync(join(directory, name));
      }
    }
  } catch { /* The inbox is created on the first completion. */ }

  return {
    write(record) {
      try {
        mkdirSync(directory, { recursive: true, mode: 0o700 });
        const name = createHash("sha256").update(record.sessionID + "\0" + record.turnID).digest("hex");
        const file = join(directory, name + ".json");
        const temporary = file + "." + process.pid + ".tmp";
        writeFileSync(temporary, JSON.stringify(record), { mode: 0o600 });
        renameSync(temporary, file);
      } catch { /* Observability must never interrupt the client. */ }
    },

    // A session deleted in the client takes its records along, so it leaves Agent HUD's list as it left the client's.
    forget(id) {
      try {
        for (const name of readdirSync(directory)) {
          if (!/^[a-f0-9]{64}\.json$/.test(name)) continue;
          try {
            if (JSON.parse(readFileSync(join(directory, name), "utf8")).sessionID === SOURCE + ":" + id) unlinkSync(join(directory, name));
          } catch { /* A record that cannot be read is pruned with the week's. */ }
        }
      } catch { /* There are no records yet. */ }
    },
  };
}

// OpenCode 1 and its forks.
async function server(input) {
  const turns = inbox();
  const sessions = new Map();

  function session(id) {
    let value = sessions.get(id);
    if (!value) sessions.set(id, value = {});
    return value;
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
    turns.write({
      version: 1, sessionID: SOURCE + ":" + id, workspace: value.directory || input.directory, title: value.title,
      model: reply.modelID, providerID: reply.providerID, turnID: reply.parentID,
      startedAtMs: startedAtMs || reply.createdAtMs || reply.completedAtMs, completedAtMs: Date.now(),
      navigationTarget: navigationTarget(),
    });
  }

  return {
    // Never throws or rejects: OpenCode 1.18 (and its forks) runs this inside the call that marks a session idle.
    event: async (payload) => {
      try {
        const event = payload?.event, properties = event?.properties || {}, info = properties.info || {};
        switch (event?.type) {
          case "session.created":
          case "session.updated":
            if (info.id) Object.assign(session(info.id), { child: !!info.parentID, title: info.title, directory: info.directory });
            break;
          case "session.deleted":
            if (info.id) { sessions.delete(info.id); turns.forget(info.id); }
            break;
          case "message.updated": {
            // The client updates earlier messages too, so the newest prompt and reply go by when they were created. The
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
      } catch { /* Observability must never interrupt the client. */ }
    },
  };
}

// OpenCode 2. An execution runs a session from a prompt until it settles, through tool calls, retries and queued
// prompts. It completed when it succeeded and its newest prompt, reply or compaction is a reply that stopped on its own
// and carries no error; a compaction or a shell command run on its own is no turn, and neither is a child session's
// run. OpenCode 2 runs plugins in one background service for every terminal, so a record names no terminal.
function setup(ctx) {
  if (!ctx?.event?.subscribe || !ctx.session) return;
  const turns = inbox(), started = new Map(), controller = new AbortController();
  // Messages a person or OpenCode put in a session, as against marks such as when it went idle.
  const prompts = ["user", "synthetic", "skill", "shell"], entries = [...prompts, "assistant", "compaction"];

  async function complete(id, at) {
    const startedAtMs = started.get(id);
    started.delete(id);
    // Each folder OpenCode serves loads the plugin, and each sees every session: the folder a session runs in records it.
    const info = await ctx.session.get({ sessionID: id }), directory = info?.location?.directory || ctx.location?.directory;
    if (!info || info.parentID || (ctx.location?.directory && directory !== ctx.location.directory)) return;
    // A prompt sent after the execution ended belongs to the next one.
    const messages = (await ctx.session.context({ sessionID: id })).filter((message) => !(message.time?.created > at));
    const last = messages.findLastIndex((message) => entries.includes(message.type));
    const reply = messages[last];
    if (reply?.type !== "assistant" || reply.error || reply.finish !== "stop" || !reply.time?.completed) return;
    const prompt = messages.slice(0, last).findLast((message) => prompts.includes(message.type));
    turns.write({
      version: 1, sessionID: SOURCE + ":" + id, workspace: directory,
      title: info.title, model: reply.model?.id, providerID: reply.model?.providerID, turnID: prompt?.id || reply.id,
      startedAtMs: startedAtMs || info.time?.busy || prompt?.time?.created || reply.time.created, completedAtMs: Date.now(),
    });
  }

  (async () => {
    const types = ["session.execution.started", "session.execution.succeeded", "session.execution.failed",
      "session.execution.interrupted", "session.deleted"];
    for await (const event of ctx.event.subscribe({ types, signal: controller.signal })) {
      try {
        const id = event?.data?.sessionID;
        if (!id) continue;
        if (event.type === "session.execution.started") started.set(id, event.created || Date.now());
        else if (event.type === "session.execution.succeeded") complete(id, event.created || Date.now()).catch(() => {});
        else if (event.type === "session.deleted") { started.delete(id); turns.forget(id); }
        else started.delete(id);
      } catch { /* Observability must never interrupt the client. */ }
    }
  })().catch(() => {});
  return () => controller.abort();
}
