// SOURCE, NAME, FOLDER, VARIABLE and ENDS, above, name the client: its sessions' id prefix, its name, its folder in
// the home folder, the variable that moves it, and the event that ends a run.
import { mkdirSync, writeFileSync, renameSync, readdirSync, statSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createHash, randomUUID } from "node:crypto";

export default function (pi) {
  const directory = join((VARIABLE && process.env[VARIABLE]) || join(homedir(), FOLDER, "agent"), "agent-hud", "turns");
  let active;
  let heartbeat;
  let lastStopReason;

  function navigationTarget() {
    const id = process.env.ITERM_SESSION_ID;
    return process.env.TERM_PROGRAM === "iTerm.app" && !process.env.TMUX && id
      ? { kind: "iTermSession", id } : undefined;
  }

  function publish(ctx, state = "running", stopReason) {
    if (!active) return;
    const sessionID = SOURCE + ":" + ctx.sessionManager.getSessionId();
    const record = {
      version: 1, sessionID, sessionFile: ctx.sessionManager.getSessionFile(),
      workspace: ctx.cwd, title: ctx.sessionManager.getSessionName() || NAME,
      model: ctx.model?.id, providerID: ctx.model?.provider,
      turnID: active.id, state, startedAtMs: active.startedAtMs, observedAtMs: Date.now(),
      navigationTarget: navigationTarget(), stopReason,
    };
    try {
      mkdirSync(directory, { recursive: true, mode: 0o700 });
      const name = createHash("sha256").update(sessionID + "\0" + active.id).digest("hex");
      const file = join(directory, name + ".json");
      const temporary = file + "." + process.pid + ".tmp";
      writeFileSync(temporary, JSON.stringify(record), { mode: 0o600 });
      renameSync(temporary, file);
    } catch { /* Observability must never interrupt the agent loop. */ }
  }

  function finish(ctx, state, stopReason) {
    publish(ctx, state, stopReason);
    clearInterval(heartbeat);
    heartbeat = undefined;
    active = undefined;
  }

  // The last message's stop reason says how the run ended: an answer, the user's abort or an error.
  function settle(ctx) {
    finish(ctx, lastStopReason === "stop" ? "completed" : "ended", lastStopReason);
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
    if (!active) active = { id: randomUUID(), startedAtMs: Date.now() };
    if (!heartbeat) {
      heartbeat = setInterval(() => publish(ctx), 15000);
      heartbeat.unref();
    }
    lastStopReason = undefined;
    publish(ctx);
  });
  pi.on("agent_end", (event, ctx) => {
    // Oh My Pi has no agent_settled: its agent_end says whether the run goes on, and the heartbeat goes on with it.
    if (ENDS === "agent_end") {
      if (!event?.willContinue) settle(ctx);
      return;
    }
    // A Pi build without agent_settled never settles the run: its heartbeat stops here, and the run goes quiet.
    clearInterval(heartbeat);
    heartbeat = undefined;
  });
  pi.on("message_end", (event, ctx) => {
    if (event.message.role === "assistant") lastStopReason = event.message.stopReason;
    publish(ctx);
  });
  pi.on("tool_execution_start", (_event, ctx) => publish(ctx));
  pi.on("tool_execution_end", (_event, ctx) => publish(ctx));
  pi.on("agent_settled", (_event, ctx) => settle(ctx));
  pi.on("session_shutdown", (_event, ctx) => finish(ctx, "ended"));
}
