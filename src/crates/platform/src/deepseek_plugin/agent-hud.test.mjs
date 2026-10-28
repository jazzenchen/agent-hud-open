// The DeepSeek Harness plugin against a stand-in for Harness's ctx: what it
// writes in the channel folder, what it submits, and how it races the client's
// own question and approval cards.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { start, apply } from "./agent-hud.mjs";

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(check, ms = 2000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const value = check();
    if (value) return value;
    await wait(5);
  }
  throw new Error("timed out");
}

/** Harness's ctx as far as the plugin uses it. */
function harness() {
  const listeners = new Map();
  const agents = new Map();
  const effects = [];
  const ctx = {
    controller: undefined,
    on(name, fn, options) {
      const list = listeners.get(name) ?? [];
      if (options?.prepend) list.unshift(fn);
      else list.push(fn);
      listeners.set(name, list);
      return () => list.splice(list.indexOf(fn), 1);
    },
    effect(fn) {
      effects.push(fn());
    },
    get(key) {
      return key === "sessionController" ? ctx.controller : undefined;
    },
    agents: {
      list: () => [...agents.values()],
      get: (id) => agents.get(id),
    },
  };
  return {
    ctx,
    agents,
    effects,
    /** A waterfall whose last answerer is `last`. */
    waterfall(name, input, last) {
      const list = listeners.get(name) ?? [];
      const step = (index) =>
        index < list.length
          ? Promise.resolve(list[index](input, () => step(index + 1)))
          : Promise.resolve().then(last);
      return step(0);
    },
    emit(name, ...args) {
      for (const fn of listeners.get(name) ?? []) fn(...args);
    },
  };
}

function setup(t) {
  const channel = fs.mkdtempSync(path.join(os.tmpdir(), "dsh-plugin-"));
  const h = harness();
  const plugin = start(h.ctx, channel, {
    host: 4242,
    checkInMs: 60_000,
    promptPollMs: 60_000,
    answerPollMs: 5,
  });
  t.after(() => {
    plugin.stop();
    fs.rmSync(channel, { recursive: true, force: true });
  });
  const json = (...parts) => JSON.parse(fs.readFileSync(path.join(channel, ...parts), "utf8"));
  const files = (...parts) => {
    try {
      return fs.readdirSync(path.join(channel, ...parts));
    } catch {
      return [];
    }
  };
  return { channel, h, plugin, json, files };
}

const session = "session-11111111-2222-4333-8444-555555555555";

test("apply needs an absolute channel and runs nothing without one", () => {
  const h = harness();
  apply(h.ctx, {});
  apply(h.ctx, { channel: "relative/folder" });
  assert.equal(h.effects.length, 0);
});

test("open sessions check in, subagents do not, and ended sessions say so", (t) => {
  const { h, plugin, json, files } = setup(t);
  h.agents.set(session, { id: session });
  h.agents.set("session-child", { id: "session-child", parent: { id: session } });
  plugin.tick();
  assert.deepEqual(files("sessions"), [`${session}.json`]);
  const checkIn = json("sessions", `${session}.json`);
  assert.equal(checkIn.host, 4242);
  assert.equal(checkIn.channelVersion, 1);
  assert.equal(typeof checkIn.at, "number");
  h.agents.delete(session);
  plugin.tick();
  assert.equal(typeof json("sessions", `${session}.json`).ended, "number");
});

test("a new session checks in at once, and a disposed one ends", (t) => {
  const { h, json } = setup(t);
  const agent = { id: session };
  h.emit("agent/created", { agent, source: "startup" });
  assert.equal(typeof json("sessions", `${session}.json`).at, "number");
  h.emit("agent/disposed", { agent });
  assert.equal(typeof json("sessions", `${session}.json`).ended, "number");
});

test("words left for a session are queued through the session controller", async (t) => {
  const { h, plugin, channel, json } = setup(t);
  h.agents.set(session, { id: session });
  const sent = [];
  h.ctx.controller = {
    prompt: async (input, signal) => {
      assert.ok(signal instanceof AbortSignal);
      sent.push(input);
      return { accepted: true };
    },
  };
  const dir = path.join(channel, "prompts", session);
  fs.mkdirSync(dir, { recursive: true });
  const id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
  const later = "ffffffff-bbbb-4ccc-8ddd-eeeeeeeeeeee";
  fs.writeFileSync(path.join(dir, `${later}.json`), JSON.stringify({ id: later, text: "second", at: 2, expiresAt: Date.now() + 60_000 }));
  fs.writeFileSync(path.join(dir, `${id}.json`), JSON.stringify({ id, text: "first", at: 1, expiresAt: Date.now() + 60_000 }));
  plugin.tick();
  await until(() => json("prompts", session, `${later}.json`).startedAt);
  assert.deepEqual(
    sent.map(({ requestId, sessionId, mode, content }) => [requestId, sessionId, mode, content[0].text]),
    [
      [id, session, "queue", "first"],
      [later, session, "queue", "second"],
    ],
  );
  assert.equal(typeof json("prompts", session, `${id}.json`).takenAt, "number");
  plugin.tick();
  assert.equal(sent.length, 2, "a taken prompt is never submitted again");
});

test("words wait for the controller, and refusals, cancels and expiry are written", async (t) => {
  const { h, plugin, channel, json } = setup(t);
  h.agents.set(session, { id: session });
  const dir = path.join(channel, "prompts", session);
  fs.mkdirSync(dir, { recursive: true });
  const put = (id, extra = {}) =>
    fs.writeFileSync(path.join(dir, `${id}.json`), JSON.stringify({ id, text: id, at: 1, expiresAt: Date.now() + 60_000, ...extra }));
  put("p-refused");
  plugin.tick();
  assert.equal(json("prompts", session, "p-refused.json").takenAt, undefined, "no controller yet: left waiting");
  h.ctx.controller = {
    prompt: async () => {
      throw new Error("session is closed");
    },
  };
  put("p-cancelled");
  fs.writeFileSync(path.join(dir, "p-cancelled.cancel"), "{}");
  put("p-expired", { expiresAt: 1 });
  plugin.tick();
  await until(() => json("prompts", session, "p-refused.json").refused);
  assert.match(json("prompts", session, "p-refused.json").refused, /session is closed/);
  assert.equal(typeof json("prompts", session, "p-cancelled.json").cancelledAt, "number");
  assert.equal(typeof json("prompts", session, "p-expired.json").expiredAt, "number");
});

test("a call to Agent HUD's MCP tools leaves a note of its caller while it runs", async (t) => {
  const { h, files, json } = setup(t);
  const exec = {
    name: "mcp__agent-hud__send_message",
    arguments: { to: "x", text: "hi" },
    agent: { id: session },
    callId: "call-1",
  };
  let seen;
  await h.waterfall("tools/execute", exec, () => {
    seen = files("calls", "4242").map((file) => json("calls", "4242", file));
    return { content: [] };
  });
  assert.equal(seen.length, 1);
  assert.equal(seen[0].tool, "send_message");
  assert.equal(seen[0].sessionId, session);
  assert.deepEqual(seen[0].arguments, { to: "x", text: "hi" });
  assert.deepEqual(files("calls", "4242"), [], "the note goes when the call ends");
  await h.waterfall("tools/execute", { ...exec, name: "bash" }, () => {
    assert.deepEqual(files("calls", "4242"), [], "other tools leave no note");
  });
});

function question(signal) {
  return {
    questions: [
      {
        id: "q1",
        question: "Which color?",
        header: "Color",
        options: [{ label: "Blue", description: "Calm" }, { label: "Red" }],
      },
    ],
    agent: { id: session },
    signal,
  };
}

test("the HUD's answer wins a question and takes the client's card away", async (t) => {
  const { h, channel, files, json } = setup(t);
  const request = question(new AbortController().signal);
  let card;
  const asked = h.waterfall("user-questions/request", request, () => {
    card = request.signal;
    return new Promise((_, reject) =>
      request.signal.addEventListener("abort", () => reject(request.signal.reason)),
    );
  });
  const name = await until(() => files("asks").find((file) => file.endsWith(".json")));
  const ask = json("asks", name);
  assert.equal(ask.kind, "question");
  assert.equal(ask.sessionId, session);
  assert.equal(ask.host, 4242);
  assert.equal(ask.timed, false);
  assert.deepEqual(ask.questions[0].options, [{ label: "Blue", description: "Calm" }, { label: "Red" }]);
  fs.writeFileSync(
    path.join(channel, "asks", name.replace(".json", ".answer")),
    JSON.stringify({ answers: [{ id: "q1", selected: ["Red"] }] }),
  );
  assert.deepEqual(await asked, { answers: [{ id: "q1", selected: ["Red"] }] });
  assert.equal(card.aborted, true, "the client's card was withdrawn");
  await until(() => files("asks").length === 0);
});

test("the client's answer wins a question, and the HUD's card goes away", async (t) => {
  const { h, files } = setup(t);
  let answer;
  const asked = h.waterfall("user-questions/request", question(new AbortController().signal), () =>
    new Promise((resolve) => {
      answer = resolve;
    }),
  );
  await until(() => answer && files("asks").length === 1);
  answer({ answers: [{ id: "q1", selected: ["Blue"] }] });
  assert.deepEqual(await asked, { answers: [{ id: "q1", selected: ["Blue"] }] });
  assert.deepEqual(files("asks"), []);
});

test("a wrong answer from the HUD is ignored", async (t) => {
  const { h, channel, files } = setup(t);
  let answer;
  const asked = h.waterfall("user-questions/request", question(new AbortController().signal), () =>
    new Promise((resolve) => {
      answer = resolve;
    }),
  );
  const name = await until(() => files("asks").find((file) => file.endsWith(".json")));
  fs.writeFileSync(path.join(channel, "asks", name.replace(".json", ".answer")), JSON.stringify({ answers: [{ id: "other", selected: [] }] }));
  await until(() => !files("asks").some((file) => file.endsWith(".answer")));
  answer({ answers: [{ id: "q1", selected: ["Blue"] }] });
  assert.equal((await asked).answers[0].selected[0], "Blue");
});

test("with no client to answer, the HUD answers while the app watches", async (t) => {
  const { h, channel, files } = setup(t);
  const nobody = () => Promise.reject(Object.assign(new Error("no answerer"), { code: "NO_PROVIDER" }));
  await assert.rejects(h.waterfall("user-questions/request", question(new AbortController().signal), nobody), /no answerer/);
  fs.writeFileSync(path.join(channel, "app.json"), JSON.stringify({ at: Date.now() }));
  const asked = h.waterfall("user-questions/request", question(new AbortController().signal), nobody);
  const name = await until(() => files("asks").find((file) => file.endsWith(".json")));
  fs.writeFileSync(path.join(channel, "asks", name.replace(".json", ".answer")), JSON.stringify({ answers: [{ id: "q1", selected: [], custom: "green" }] }));
  assert.deepEqual(await asked, { answers: [{ id: "q1", selected: [], custom: "green" }] });
});

test("a cancelled question ends the HUD's card too", async (t) => {
  const { h, files } = setup(t);
  const controller = new AbortController();
  const request = question(controller.signal);
  let listening = false;
  const asked = h.waterfall("user-questions/request", request, () =>
    new Promise((_, reject) => {
      request.signal.addEventListener("abort", () => reject(new Error("aborted")));
      listening = true;
    }),
  );
  await until(() => listening && files("asks").length === 1);
  controller.abort();
  await assert.rejects(asked, /aborted/);
  assert.deepEqual(files("asks"), []);
});

test("an approval shows the call it is about, and the HUD can reject it", async (t) => {
  const { h, channel, files, json } = setup(t);
  await h.waterfall(
    "tools/pre-execute",
    { name: "bash", callId: "call-7", arguments: { command: "rm -rf build" }, agent: { id: session } },
    () => ({ kind: "allow" }),
  );
  const request = {
    agent: { id: session },
    toolName: "bash",
    callId: "call-7",
    reason: "escalate sandbox to workspace-write: tidy",
    displayReason: { en: "Allow this operation", zh: "允许本次操作" },
    signal: new AbortController().signal,
  };
  let card;
  const decided = h.waterfall("approval/request", request, () => {
    card = request.signal;
    return new Promise((resolve) => request.signal.addEventListener("abort", () => resolve("cancelled")));
  });
  const name = await until(() => files("asks").find((file) => file.endsWith(".json")));
  const ask = json("asks", name);
  assert.equal(ask.kind, "approval");
  assert.equal(ask.toolName, "bash");
  assert.equal(ask.arguments, JSON.stringify({ command: "rm -rf build" }));
  assert.deepEqual(ask.displayReason, { en: "Allow this operation", zh: "允许本次操作" });
  fs.writeFileSync(path.join(channel, "asks", name.replace(".json", ".answer")), JSON.stringify("maybe"));
  await wait(30);
  fs.writeFileSync(path.join(channel, "asks", name.replace(".json", ".answer")), JSON.stringify("rejected"));
  assert.equal(await decided, "rejected");
  assert.equal(card.aborted, true);
});

test("a subagent's question and a host that is stopping pass straight through", async (t) => {
  const { h, plugin, files } = setup(t);
  const child = { ...question(new AbortController().signal), agent: { id: "session-child", parent: {} } };
  assert.equal(await h.waterfall("user-questions/request", child, () => "client"), "client");
  plugin.stop();
  assert.equal(await h.waterfall("user-questions/request", question(new AbortController().signal), () => "client"), "client");
  assert.deepEqual(files("asks"), []);
});

test("a broken agents service never throws out of a timer", (t) => {
  const { h, plugin } = setup(t);
  h.ctx.agents.list = () => {
    throw new Error("agents are gone");
  };
  plugin.tick();
  h.ctx.agents = undefined;
  plugin.tick();
});
