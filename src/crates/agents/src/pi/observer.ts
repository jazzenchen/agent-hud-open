// SOURCE, NAME, FOLDER, VARIABLE and ENDS, above, name the client: its sessions' id prefix, its name, its folder in
// the home folder, the variable that moves it, and the event that ends a run. Messages other agents leave for the
// session come in through `agentHudMailbox` (before this), as the user's next prompt once the session is idle.
import { mkdirSync, writeFileSync, renameSync, readdirSync, statSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createHash, randomUUID } from "node:crypto";

export default function (pi) {
  const directory = join((VARIABLE && process.env[VARIABLE]) || join(homedir(), FOLDER, "agent"), "agent-hud", "turns");
  let active;
  let heartbeat;
  let lastStopReason;
  const mailbox = agentHudMailbox(SOURCE);
  let context, look, said = 0, delivered, lastText;

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
      navigationTarget: navigationTarget(), stopReason, delivered: active.delivered,
      message: state === "completed" ? lastText : undefined,
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

  // Hands the oldest waiting message to the session while it is idle, as `pi.sendUserMessage` starts a run with it;
  // that run's records name the message (`delivered`). Says now and then which session this process holds.
  function mail() {
    const ctx = context;
    if (!ctx) return;
    try {
      const id = ctx.sessionManager.getSessionId();
      if (Date.now() - said > 20000) { said = Date.now(); mailbox.present([id]); }
      if (active || delivered || !ctx.isIdle()) return;
      const message = mailbox.take(id);
      if (!message) return;
      delivered = message.id;
      pi.sendUserMessage(message.text).catch(() => { delivered = undefined; });
    } catch { /* The next look tries again. */ }
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

  pi.on("session_start", (_event, ctx) => {
    context = ctx;
    said = 0;
    if (!look) {
      look = setInterval(mail, 2000);
      look.unref();
    }
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
    if (!active) active = { id: randomUUID(), startedAtMs: Date.now(), delivered };
    delivered = undefined;
    context = ctx;
    if (!heartbeat) {
      heartbeat = setInterval(() => publish(ctx), 15000);
      heartbeat.unref();
    }
    lastStopReason = undefined;
    lastText = undefined;
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
    if (event.message.role === "assistant") {
      lastStopReason = event.message.stopReason;
      // The reply's words, for whoever sent the message this turn answers.
      const content = event.message.content;
      const text = typeof content === "string" ? content : Array.isArray(content)
        ? content.filter((part) => part?.type === "text" && typeof part.text === "string").map((part) => part.text).join("") : "";
      if (text.trim()) lastText = text.slice(0, 40000);
    }
    publish(ctx);
  });
  pi.on("tool_execution_start", (event, ctx) => {
    // A call to Agent HUD's own MCP tools names its session, so Agent HUD knows who sends a message.
    try { mailbox.calling(ctx.sessionManager.getSessionId(), mailbox.tool(event?.toolName), event?.args, event?.toolCallId); } catch { /* Never interrupts the call. */ }
    publish(ctx);
  });
  pi.on("tool_execution_end", (event, ctx) => {
    if (mailbox.tool(event?.toolName)) mailbox.called(event?.toolCallId);
    publish(ctx);
  });
  pi.on("agent_settled", (_event, ctx) => {
    settle(ctx);
    mail();
  });
  pi.on("session_shutdown", (_event, ctx) => {
    finish(ctx, "ended");
    clearInterval(look);
    look = context = undefined;
    mailbox.absent();
  });
}
