// Agent HUD's plugin for DeepSeek Harness. Agent HUD writes this file and one
// row in $DSH_HOME/cordis.patch.yml that loads it into every Harness host,
// the Desktop app's included; taking the row out unloads it.
//
// It shares one folder with the app, `config.channel`:
// - sessions/<session>.json: every open session checks in every 5 seconds;
// - prompts/<session>/<id>.json: words the app leaves for a session, which
//   this plugin queues as the session's next message;
// - calls/<host>/<id>.json: which session is calling one of Agent HUD's MCP
//   tools right now, so the app can tell who called;
// - asks/<id>.json: a question or approval a session waits on. The HUD may
//   answer it first, in asks/<id>.answer; whichever answer comes first wins,
//   and the other side's card goes away.
//
// It uses only ctx and Node's own modules: a package import fails outside a
// profile. A throw that escapes a timer or a promise stops the whole Harness
// host, the Desktop app's too, and the Desktop loads this file again at every
// launch. So every callback catches its own errors, and every service is
// looked up when it is used and checked first.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

export const name = "agent-hud";
export const inject = ["agents"];

/** The folder layout this file writes; the app reads it as `channelVersion`. */
export const CHANNEL_VERSION = 1;
const CHECK_IN_MS = 5_000;
const PROMPT_POLL_MS = 1_000;
const ANSWER_POLL_MS = 250;
/** The app writes app.json while it watches asks; older than this, it is gone. */
const APP_LIFETIME_MS = 10_000;
/** How much of a tool call's arguments an approval card carries. */
const ARGUMENTS_LIMIT = 8_000;
const MCP_PREFIX = "mcp__agent-hud__";
const NAME = /^[A-Za-z0-9_-]{1,128}$/;

export function apply(ctx, config) {
  const channel = config?.channel;
  if (typeof channel !== "string" || !path.isAbsolute(channel)) return;
  const plugin = start(ctx, channel);
  ctx.effect(() => () => plugin.stop());
}

/** Runs the plugin against `channel`; `stop` ends it. Exported for tests. */
export function start(ctx, channel, options = {}) {
  const now = options.now ?? Date.now;
  const host = options.host ?? process.pid;
  const timing = {
    checkIn: options.checkInMs ?? CHECK_IN_MS,
    prompts: options.promptPollMs ?? PROMPT_POLL_MS,
    answers: options.answerPollMs ?? ANSWER_POLL_MS,
  };
  const folder = (...parts) => path.join(channel, ...parts);
  const timers = new Set();
  const checkedIn = new Set();
  const asks = new Map();
  const calls = new Map();
  const queues = new Map();
  const notes = new Set();
  let stopped = false;
  let answerTimer = null;

  /** Runs `work`, sync or async, and keeps any error inside. */
  const quietly = (work) => {
    try {
      const result = work();
      if (result && typeof result.then === "function")
        result.then(undefined, () => {});
    } catch {}
  };
  const every = (ms, work) => {
    const timer = setInterval(() => quietly(work), ms);
    timer.unref?.();
    timers.add(timer);
    return timer;
  };
  const service = (key) => {
    try {
      return ctx.get?.(key);
    } catch {
      return undefined;
    }
  };

  // ---- Files ----
  const directory = (dir) => fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const write = (file, value) => {
    directory(path.dirname(file));
    const temp = `${file}.${crypto.randomUUID()}.tmp`;
    try {
      fs.writeFileSync(temp, JSON.stringify(value), { mode: 0o600 });
      fs.renameSync(temp, file);
    } catch (error) {
      try {
        fs.rmSync(temp, { force: true });
      } catch {}
      throw error;
    }
  };
  const read = (file) => {
    try {
      return JSON.parse(fs.readFileSync(file, "utf8"));
    } catch {
      return undefined;
    }
  };
  const remove = (file) => {
    try {
      fs.rmSync(file, { force: true, recursive: true });
    } catch {}
  };

  // ---- Sessions ----
  const agents = () => {
    try {
      const list = ctx.agents?.list?.();
      return Array.isArray(list) ? list : [];
    } catch {
      return [];
    }
  };
  /** A session the user talks to: a root agent, not a subagent. */
  const root = (agent) =>
    agent && typeof agent.id === "string" && NAME.test(agent.id) && !agent.parent;
  const roots = () => agents().filter(root);
  const checkIn = (id) => {
    write(folder("sessions", `${id}.json`), {
      at: now(),
      host,
      channelVersion: CHANNEL_VERSION,
    });
    checkedIn.add(id);
  };
  const end = (id) => {
    if (!checkedIn.delete(id)) return;
    quietly(() => write(folder("sessions", `${id}.json`), { ended: now() }));
  };
  const checkIns = () => {
    const open = new Set();
    for (const agent of roots()) {
      open.add(agent.id);
      quietly(() => checkIn(agent.id));
    }
    for (const id of [...checkedIn]) if (!open.has(id)) end(id);
  };

  // ---- Words the app leaves for a session ----
  const submit = (sessionId, file, prompt) => {
    const controller = service("sessionController");
    if (typeof controller?.prompt !== "function") return;
    const takenAt = now();
    // Claimed before it is submitted, so a reload never submits it twice.
    write(file, { takenAt });
    const became = (what) =>
      quietly(() => write(file, { takenAt, ...what }));
    const previous = queues.get(sessionId) ?? Promise.resolve();
    const next = previous
      .then(() =>
        controller.prompt(
          {
            requestId: String(prompt.id),
            sessionId,
            mode: "queue",
            content: [{ type: "text", text: prompt.text }],
          },
          new AbortController().signal,
        ),
      )
      .then(
        (result) =>
          became(
            result?.accepted === false
              ? { refused: String(result?.reason ?? "refused") }
              : { startedAt: now() },
          ),
        (error) => became({ refused: String(error?.message ?? error) }),
      );
    queues.set(sessionId, next);
    next.then(() => {
      if (queues.get(sessionId) === next) queues.delete(sessionId);
    });
  };
  const prompts = () => {
    for (const agent of roots()) {
        const dir = folder("prompts", agent.id);
        let names;
        try {
          names = fs.readdirSync(dir).filter((entry) => entry.endsWith(".json"));
        } catch {
          continue;
        }
        const pending = [];
        for (const entry of names) {
          const file = path.join(dir, entry);
          const prompt = read(file);
          if (
            !prompt ||
            typeof prompt.text !== "string" ||
            !NAME.test(String(prompt.id)) ||
            prompt.takenAt !== undefined ||
            prompt.cancelledAt !== undefined ||
            prompt.expiredAt !== undefined
          )
            continue;
          pending.push({ file, prompt });
        }
        pending.sort(
          (a, b) =>
            Number(a.prompt.at) - Number(b.prompt.at) ||
            String(a.prompt.id).localeCompare(String(b.prompt.id)),
        );
        for (const { file, prompt } of pending)
          quietly(() => {
            if (fs.existsSync(file.slice(0, -5) + ".cancel"))
              return write(file, { cancelledAt: now() });
            if (typeof prompt.expiresAt === "number" && prompt.expiresAt <= now())
              return write(file, { expiredAt: now() });
            submit(agent.id, file, prompt);
          });
    }
  };

  // ---- Who calls Agent HUD's MCP tools ----
  const callNote = (exec) => {
    const tool = typeof exec?.name === "string" ? exec.name : "";
    const sessionId = exec?.agent?.id;
    if (!tool.startsWith(MCP_PREFIX) || typeof sessionId !== "string") return null;
    const file = folder("calls", String(host), `${crypto.randomUUID()}.json`);
    write(file, {
      tool: tool.slice(MCP_PREFIX.length),
      arguments: exec.arguments ?? {},
      sessionId,
      at: now(),
    });
    notes.add(file);
    return file;
  };
  /** The call an approval is about, so its card can show what it does. */
  const remember = (exec) => {
    const callId = exec?.callId;
    if (typeof callId !== "string" || typeof exec?.name !== "string") return;
    let text = "";
    try {
      text = JSON.stringify(exec.arguments ?? {});
    } catch {}
    calls.set(callId, {
      name: exec.name,
      arguments: text.length > ARGUMENTS_LIMIT ? text.slice(0, ARGUMENTS_LIMIT) : text,
    });
    while (calls.size > 200) calls.delete(calls.keys().next().value);
  };

  // ---- Questions and approvals ----
  const appWatching = () => {
    const app = read(folder("app.json"));
    return typeof app?.at === "number" && now() - app.at < APP_LIFETIME_MS;
  };
  const answer = (ask, value) => {
    if (ask.kind === "approval")
      return value === "allowed-once" || value === "rejected" ? value : undefined;
    const items = value?.answers;
    if (!Array.isArray(items) || items.length !== ask.questions.length) return undefined;
    const ids = new Set(ask.questions);
    for (const item of items) {
      if (!ids.delete(item?.id)) return undefined;
      if (!Array.isArray(item.selected) || item.selected.some((s) => typeof s !== "string"))
        return undefined;
      if (item.custom !== undefined && typeof item.custom !== "string") return undefined;
    }
    return { answers: items.map(({ id, selected, custom }) => ({ id, selected, ...(custom === undefined ? {} : { custom }) })) };
  };
  const pollAnswers = () => {
    for (const [id, ask] of asks) {
      const file = folder("asks", `${id}.answer`);
      if (!fs.existsSync(file)) continue;
      const value = answer(ask, read(file));
      remove(file);
      if (value !== undefined) ask.settle(value);
    }
    if (asks.size === 0 && answerTimer) {
      clearInterval(answerTimer);
      timers.delete(answerTimer);
      answerTimer = null;
    }
  };
  /** Asks the HUD alongside the client's own card; the first answer wins. */
  const race = async (kind, request, next, body) => {
    const original = request?.signal;
    const sessionId = request?.agent?.id;
    if (stopped || original?.aborted || !root(request?.agent) || !NAME.test(sessionId))
      return next();
    const ours = new AbortController();
    try {
      // The client's card listens to this signal: aborting ours takes the
      // card away when the HUD answers first.
      request.signal = original ? AbortSignal.any([original, ours.signal]) : ours.signal;
    } catch {}
    if (request.signal === original) return next();
    const id = crypto.randomUUID();
    const file = folder("asks", `${id}.json`);
    let settle;
    const fromHud = new Promise((resolve) => {
      settle = resolve;
    });
    try {
      write(file, { id, kind, sessionId, host, at: now(), ...body });
    } catch {
      return next();
    }
    asks.set(id, { kind, questions: (body.questions ?? []).map((q) => q.id), settle });
    if (!answerTimer) answerTimer = every(timing.answers, pollAnswers);
    const onAbort = () => settle(undefined);
    original?.addEventListener?.("abort", onAbort, { once: true });
    try {
      const fromClient = Promise.resolve().then(next);
      fromClient.then(undefined, () => {});
      const first = await Promise.race([
        fromClient.then(
          (value) => ({ value }),
          (error) => ({ error }),
        ),
        fromHud.then((value) => (value === undefined ? new Promise(() => {}) : { value, hud: true })),
      ]);
      if (first.hud) {
        ours.abort(new Error("Answered in Agent HUD"));
        return first.value;
      }
      if (first.error === undefined) return first.value;
      // No client answers here (a host without a window): the HUD still can,
      // while the app is there to show it.
      const nobody = first.error?.code === "NO_PROVIDER";
      if (!nobody || original?.aborted || !appWatching()) throw first.error;
      const value = await fromHud;
      if (value === undefined) throw first.error;
      return value;
    } finally {
      original?.removeEventListener?.("abort", onAbort);
      asks.delete(id);
      remove(file);
      remove(folder("asks", `${id}.answer`));
    }
  };
  const questionBody = (request) => ({
    questions: (Array.isArray(request?.questions) ? request.questions : []).map((q) => ({
      id: String(q?.id ?? ""),
      question: String(q?.question ?? ""),
      ...(typeof q?.header === "string" ? { header: q.header } : {}),
      ...(typeof q?.detail === "string" ? { detail: q.detail } : {}),
      options: (Array.isArray(q?.options) ? q.options : []).map((o) => ({
        label: String(o?.label ?? ""),
        ...(typeof o?.description === "string" ? { description: o.description } : {}),
      })),
      multiSelect: q?.multiSelect === true,
      ...(typeof q?.intent?.kind === "string" ? { intent: { kind: q.intent.kind } } : {}),
    })),
    timed: request?.wait?.timed === true,
  });
  const approvalBody = (request) => {
    const call = typeof request?.callId === "string" ? calls.get(request.callId) : undefined;
    const reason = request?.displayReason;
    return {
      toolName: String(request?.toolName ?? ""),
      ...(typeof request?.callId === "string" ? { callId: request.callId } : {}),
      ...(typeof request?.reason === "string" ? { reason: request.reason } : {}),
      ...(reason && typeof reason === "object"
        ? { displayReason: { en: String(reason.en ?? ""), zh: String(reason.zh ?? "") } }
        : {}),
      ...(call ? { arguments: call.arguments } : {}),
    };
  };

  // ---- Wiring ----
  const first = { prepend: true };
  ctx.on(
    "user-questions/request",
    (request, next) => {
      let body;
      try {
        body = questionBody(request);
      } catch {
        return next();
      }
      return race("question", request, next, body);
    },
    first,
  );
  ctx.on(
    "approval/request",
    (request, next) => {
      let body;
      try {
        body = approvalBody(request);
      } catch {
        return next();
      }
      return race("approval", request, next, body);
    },
    first,
  );
  ctx.on("tools/pre-execute", (exec, next) => {
    quietly(() => remember(exec));
    return next();
  });
  ctx.on("tools/execute", async (exec, next) => {
    let note = null;
    try {
      note = callNote(exec);
    } catch {}
    try {
      return await next();
    } finally {
      if (note) {
        notes.delete(note);
        remove(note);
      }
    }
  });
  ctx.on("tools/result", (exec) => {
    quietly(() => {
      if (typeof exec?.callId === "string") calls.delete(exec.callId);
    });
  });
  ctx.on("agent/created", (event) => {
    quietly(() => {
      if (root(event?.agent)) checkIn(event.agent.id);
    });
    return undefined;
  });
  ctx.on("agent/disposed", (event) => {
    quietly(() => {
      if (typeof event?.agent?.id === "string") end(event.agent.id);
    });
  });

  quietly(checkIns);
  every(timing.checkIn, checkIns);
  every(timing.prompts, prompts);

  return {
    /** Ends every timer, and says the sessions here are gone. */
    stop() {
      if (stopped) return;
      stopped = true;
      for (const timer of timers) clearInterval(timer);
      timers.clear();
      for (const ask of asks.values()) ask.settle(undefined);
      for (const id of [...checkedIn]) end(id);
      for (const note of notes) remove(note);
      notes.clear();
    },
    /** Runs one round of each timer now, for tests. */
    tick() {
      quietly(checkIns);
      quietly(prompts);
      quietly(pollAnswers);
    },
  };
}
