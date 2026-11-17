// OpenCode's plugin against stand-ins for OpenCode 2's plugin context and OpenCode 1's client: which sessions are
// handed the messages other agents leave, that one message goes in once, and what the turn it starts records.
import test, { mock } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = fs.mkdtempSync(path.join(os.tmpdir(), "opencode-plugin-"));
test.after(() => fs.rmSync(root, { recursive: true, force: true }));

// The plugin as mod.rs joins it, with Agent HUD's channel folder written in, for a test of its own: its channel, its
// data folder (where turns are recorded) and both OpenCode versions' plugins.
let made = 0;
async function plugin() {
  const folder = path.join(root, String(++made));
  const channel = path.join(folder, "hook-channel");
  fs.mkdirSync(channel, { recursive: true });
  const file = path.join(folder, "agent-hud.mjs");
  fs.writeFileSync(
    file,
    [
      'const SOURCE = "opencode", FOLDER = "opencode";\n',
      `const CHANNEL = ${JSON.stringify(channel)};\n`,
      fs.readFileSync(path.join(here, "../hooks/mailbox.js"), "utf8"),
      fs.readFileSync(path.join(here, "observer.js"), "utf8"),
      "export { server, setup };\n",
    ].join(""),
  );
  process.env.XDG_DATA_HOME = path.join(folder, "data");
  return { ...(await import(pathToFileURL(file).href)), channel, data: process.env.XDG_DATA_HOME };
}

// A message another agent left for `session`, as Agent HUD writes it.
function leave(channel, session, id, text, at = Date.now()) {
  const box = path.join(channel, "prompts", "opencode--" + session);
  fs.mkdirSync(box, { recursive: true });
  fs.writeFileSync(path.join(box, id + ".json"), JSON.stringify({ text, at }));
}
const letter = (channel, session, id) =>
  JSON.parse(fs.readFileSync(path.join(channel, "prompts", "opencode--" + session, id + ".json"), "utf8"));
const read = (folder) =>
  fs.existsSync(folder)
    ? fs.readdirSync(folder).filter((name) => name.endsWith(".json"))
      .map((name) => JSON.parse(fs.readFileSync(path.join(folder, name), "utf8")))
    : [];
const presence = (channel) => read(path.join(channel, "observers"));
const records = (data) => read(path.join(data, "opencode", "agent-hud", "turns"));

const flush = async () => {
  for (let i = 0; i < 30; i++) await new Promise((resolve) => setImmediate(resolve));
};
// One look of every copy of the plugin: the mailbox looks every two seconds.
async function look() {
  mock.timers.tick(2000);
  await flush();
}

/** OpenCode 2's background service as far as the plugin uses it: one store of sessions, a context per folder. */
function service() {
  const sessions = new Map(), history = new Map(), subscribers = new Set(), prompts = [];
  let clock = 1_800_000_000_000, count = 0;
  const world = {
    prompts,
    session(id, directory, extra = {}) {
      const at = ++clock;
      sessions.set(id, { id, location: { directory }, title: id, time: { created: at, updated: at }, ...extra });
      history.set(id, []);
    },
    info: (id) => sessions.get(id),
    add(id, message) {
      history.get(id).push({ time: { created: ++clock }, ...message });
    },
    emit(type, sessionID, data = {}) {
      const event = { type, created: ++clock, data: { sessionID, ...data } };
      for (const subscriber of subscribers) subscriber(event);
      return event.created;
    },
    // An execution in `id`: started, its reply, ended as `end`; the session's times as OpenCode 2 keeps them.
    async run(id, reply, end = "session.execution.succeeded", data) {
      const session = sessions.get(id);
      session.time.busy = world.emit("session.execution.started", id);
      if (reply) world.add(id, { ...reply, time: { created: ++clock, completed: ++clock } });
      session.time.idle = ++clock;
      world.add(id, { type: "idle", id: "msg_idle" + ++count, outcome: "succeeded" });
      world.emit(end, id, data);
      await flush();
    },
    ctx(directory) {
      return {
        location: { directory },
        event: {
          subscribe({ types, signal }) {
            const queue = [], waiting = [];
            const push = (event) => {
              if (!types.includes(event.type)) return;
              const next = waiting.shift();
              if (next) next({ value: event, done: false });
              else queue.push(event);
            };
            subscribers.add(push);
            signal?.addEventListener("abort", () => {
              subscribers.delete(push);
              for (const next of waiting.splice(0)) next({ value: undefined, done: true });
            });
            return {
              [Symbol.asyncIterator]: () => ({
                next: () =>
                  queue.length ? Promise.resolve({ value: queue.shift(), done: false })
                    : new Promise((resolve) => waiting.push(resolve)),
                return: async () => ({ value: undefined, done: true }),
              }),
            };
          },
        },
        session: {
          async list(input) {
            const data = [...sessions.values()]
              .filter((s) => s.location.directory === input.directory && (input.parentID !== null || !s.parentID))
              .sort((a, b) => b.time.updated - a.time.updated)
              .slice(0, input.limit);
            return { data, cursor: {} };
          },
          async get({ sessionID }) {
            const info = sessions.get(sessionID);
            if (!info) throw new Error("Session not found");
            return structuredClone(info);
          },
          async context({ sessionID }) {
            return structuredClone(history.get(sessionID) || []);
          },
          async prompt(input) {
            prompts.push(input);
            const id = "msg_prompt" + ++count;
            world.add(input.sessionID, { type: "user", id, text: input.text, metadata: input.metadata });
            return { id, sessionID: input.sessionID, type: "user" };
          },
        },
      };
    },
  };
  return world;
}
const reply = (text, extra = {}) => ({
  type: "assistant", id: "msg_reply" + text.length, finish: "stop", model: { id: "mock-1", providerID: "mock" },
  content: [{ type: "reasoning", text: "Thinking." }, { type: "text", text }], ...extra,
});

test("OpenCode 2 hands a waiting message to an idle main session of the folder it serves, once", async (t) => {
  mock.timers.enable({ apis: ["setInterval"] });
  t.after(() => mock.timers.reset());
  const { setup, channel, data } = await plugin();
  const world = service();
  world.session("ses_busy", "/repo");
  world.session("ses_a", "/repo");
  world.session("ses_child", "/repo", { parentID: "ses_a" });
  world.session("ses_undo", "/repo", { revert: { messageID: "msg_x" } });
  world.session("ses_far", "/other");
  world.info("ses_busy").time.busy = Date.now();
  leave(channel, "ses_a", "m-a1", "Port the parser", 1);
  leave(channel, "ses_a", "m-a2", "Then review it", 2);
  for (const id of ["ses_busy", "ses_child", "ses_undo", "ses_far"]) leave(channel, id, "m-" + id, "Hello " + id);
  // Each folder OpenCode serves loads the plugin; two copies serve the same one.
  const stops = [setup(world.ctx("/repo")), setup(world.ctx("/repo")), setup(world.ctx("/other"))];

  await look();
  // Each copy names the main sessions of its own folder, newest first.
  const named = presence(channel).map((note) => note.sessions.join(",")).sort();
  assert.deepEqual(named, ["ses_far", "ses_undo,ses_a,ses_busy", "ses_undo,ses_a,ses_busy"]);
  // The oldest message, once, as a queued prompt marked with its id; the other folder's copy took its own.
  const into = (id) => world.prompts.filter((p) => p.sessionID === id);
  assert.deepEqual(into("ses_a"), [{
    sessionID: "ses_a", text: "Port the parser", metadata: { agentHud: { message: "m-a1" } }, delivery: "queue",
  }]);
  assert.equal(into("ses_far").length, 1);
  assert.equal(letter(channel, "ses_a", "m-a1").takenAt > 0, true);
  assert.equal(letter(channel, "ses_a", "m-a2").takenAt, undefined);
  // A busy session, a child, and one with an undo pending wait.
  for (const id of ["ses_busy", "ses_child", "ses_undo"]) assert.equal(into(id).length, 0, id);

  // The next message waits for the execution the first started.
  await look();
  assert.equal(into("ses_a").length, 1);
  await world.run("ses_a", reply("Ported."));
  await look();
  assert.deepEqual(into("ses_a").map((p) => p.text), ["Port the parser", "Then review it"]);

  // The busy session takes its message once its execution ends.
  world.info("ses_busy").time.idle = Date.now() + 1;
  await look();
  assert.equal(into("ses_busy").length, 1);
  assert.equal(into("ses_child").length, 0);

  for (const stop of stops) await stop();
  assert.deepEqual(presence(channel), []);
  assert.equal(records(data).length, 1);
});

test("OpenCode 2's turn names the message that started it and carries its reply", async (t) => {
  mock.timers.enable({ apis: ["setInterval"] });
  t.after(() => mock.timers.reset());
  const { setup, channel, data } = await plugin();
  const world = service();
  world.session("ses_a", "/repo");
  leave(channel, "ses_a", "m-a1", "Port the parser");
  // A copy that did not hand it over records the same turn.
  const stops = [setup(world.ctx("/repo")), setup(world.ctx("/repo")), setup(world.ctx("/other"))];
  await look();
  await world.run("ses_a", reply("Ported."));
  const [record] = records(data);
  assert.equal(records(data).length, 1);
  assert.equal(record.sessionID, "opencode:ses_a");
  assert.equal(record.delivered, "m-a1");
  assert.equal(record.message, "Ported.");
  assert.equal(record.turnID, "msg_prompt1");
  assert.equal(record.workspace, "/repo");

  // A turn the person starts afterwards names no message.
  world.add("ses_a", { type: "user", id: "msg_typed", text: "Thanks" });
  await world.run("ses_a", reply("You are welcome."));
  const typed = records(data).find((r) => r.turnID === "msg_typed");
  assert.equal(typed.delivered, undefined);
  assert.equal(typed.message, "You are welcome.");
  for (const stop of stops) await stop();
});

test("OpenCode 2 records a turn that took a message and failed or was stopped as ended", async (t) => {
  mock.timers.enable({ apis: ["setInterval"] });
  t.after(() => mock.timers.reset());
  const { setup, channel, data } = await plugin();
  const world = service();
  world.session("ses_a", "/repo");
  world.session("ses_b", "/repo");
  world.session("ses_c", "/repo");
  leave(channel, "ses_a", "m-a1", "Port the parser");
  leave(channel, "ses_b", "m-b1", "Review it");
  const stop = setup(world.ctx("/repo"));
  await look();
  await world.run("ses_a", reply("", { finish: "error", error: { type: "provider" } }), "session.execution.failed");
  await world.run("ses_b", undefined, "session.execution.interrupted", { reason: "user" });
  // One that took no message records nothing when it fails.
  world.add("ses_c", { type: "user", id: "msg_typed", text: "Hi" });
  await world.run("ses_c", undefined, "session.execution.failed");
  const ended = Object.fromEntries(records(data).map((r) => [r.sessionID, r]));
  assert.deepEqual(Object.keys(ended).sort(), ["opencode:ses_a", "opencode:ses_b"]);
  assert.deepEqual([ended["opencode:ses_a"].state, ended["opencode:ses_a"].stopReason], ["ended", "error"]);
  assert.equal(ended["opencode:ses_a"].delivered, "m-a1");
  assert.deepEqual([ended["opencode:ses_b"].state, ended["opencode:ses_b"].stopReason], ["ended", "aborted"]);
  // A shutdown ends no turn: the execution resumes when OpenCode starts again.
  leave(channel, "ses_c", "m-c1", "Again");
  await look();
  await world.run("ses_c", undefined, "session.execution.interrupted", { reason: "shutdown" });
  assert.equal(records(data).some((r) => r.sessionID === "opencode:ses_c"), false);
  await stop();
});

test("OpenCode 1 still hands messages through its server's client, on the newest prompt's model", async (t) => {
  mock.timers.enable({ apis: ["setInterval"] });
  t.after(() => mock.timers.reset());
  const { server, channel } = await plugin();
  const sent = [];
  const client = {
    session: {
      list: async () => ({ data: [
        { id: "ses_old", time: { updated: 1 } }, { id: "ses_new", time: { updated: 2 } },
        { id: "ses_child", parentID: "ses_new", time: { updated: 3 } },
      ] }),
      messages: async () => ({ data: [
        { info: { role: "user", agent: "build", model: { providerID: "mock", modelID: "mock-1" }, time: { created: 1 } } },
      ] }),
      promptAsync: async (request) => { sent.push(request); },
    },
  };
  const hooks = await server({ client, directory: "/repo" });
  // A session in a busy stretch waits.
  await hooks.event({ event: { type: "session.status", properties: { sessionID: "ses_old", status: { type: "busy" } } } });
  leave(channel, "ses_new", "m-n1", "Port the parser");
  leave(channel, "ses_old", "m-o1", "Review it");
  await look();
  assert.deepEqual(presence(channel).map((note) => note.sessions), [["ses_new", "ses_old"]]);
  assert.deepEqual(sent, [{
    path: { id: "ses_new" },
    body: { model: { providerID: "mock", modelID: "mock-1" }, agent: "build", parts: [{ type: "text", text: "Port the parser" }] },
  }]);
  await look();
  assert.equal(sent.length, 1);
});
