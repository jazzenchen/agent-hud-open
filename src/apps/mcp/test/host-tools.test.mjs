// The relay lists the host's own tools and hands each call to the host with the
// calling session as its client names it. A stand-in engine plays the app.
import { after, test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";

const relay = fileURLToPath(new URL("../src/index.ts", import.meta.url));
const tsx = import.meta.resolve("tsx");
const folder = mkdtempSync(join(tmpdir(), "agent-hud-mcp-"));
after(() => rmSync(folder, { recursive: true, force: true }));

/** An engine that answers host_tools with `tools` (or fails it) and writes down every
 * request. `tools` may be read again for each request from a file the test changes. */
function engine(name, tools, state) {
  const file = join(
    folder,
    `${name}${process.platform === "win32" ? ".exe" : ""}`,
  );
  const source = join(folder, `${name}.cjs`);
  const log = `${file}.log`;
  writeFileSync(
    source,
    `const { appendFileSync, readFileSync } = require('node:fs');
require('node:readline').createInterface({ input: process.stdin }).on('line', line => {
  const request = JSON.parse(line);
  appendFileSync(${JSON.stringify(log)}, line + '\\n');
  const tools = ${state ? `JSON.parse(readFileSync(${JSON.stringify(state)}, 'utf8'))` : JSON.stringify(tools)};
  const reply = request.method === 'host_tools'
    ? (tools ? { result: { tools } } : { error: { message: 'The app is not running' } })
    : request.method === 'host_tool' ? { result: { echoed: request.params.name } } : { error: { message: 'not here' } };
  process.stdout.write(JSON.stringify({ id: request.id, ...reply }) + '\\n');
});
`,
  );
  // The real engine is an executable on every host, including Windows where a
  // shebang script cannot be spawned as one. Use the packaging toolchain here too.
  const compiled = spawnSync(
    process.env.BUN_EXECUTABLE ?? "bun",
    ["build", "--compile", source, "--outfile", file],
    { encoding: "utf8" },
  );
  assert.equal(compiled.status, 0, compiled.error?.message ?? compiled.stderr);
  return {
    file,
    requests: () =>
      readFileSync(log, "utf8")
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line)),
  };
}
async function connect(engineFile, env = {}, options = {}) {
  const client = new Client({ name: "relay-test", version: "0.0.0" }, options);
  const { CLAUDE_CODE_SESSION_ID: _, ...rest } = process.env;
  await client.connect(
    new StdioClientTransport({
      command: process.execPath,
      args: ["--import", tsx, relay],
      env: { ...rest, AGENTHUD_NEXT_ENGINE: engineFile, ...env },
    }),
  );
  return client;
}
const tool = {
  name: "some_tool",
  description: "A tool of the host.",
  inputSchema: {
    type: "object",
    properties: { text: { type: "string" } },
    required: ["text"],
  },
  readOnly: false,
};
const reader = {
  name: "some_reader",
  description: "A reading tool of the host.",
  inputSchema: { type: "object", properties: {} },
  readOnly: true,
};

test("host tools are listed with their schema and hand the Codex caller from the call", async () => {
  const fake = engine("codex", [tool, reader]);
  const client = await connect(fake.file);
  try {
    const { tools } = await client.listTools();
    const listed = tools.find((item) => item.name === "some_tool");
    assert.equal(listed.description, "A tool of the host.");
    assert.deepEqual(listed.inputSchema.required, ["text"]);
    assert.equal(listed.annotations.readOnlyHint, false);
    assert.equal(
      tools.find((item) => item.name === "some_reader").annotations
        .readOnlyHint,
      true,
    );
    assert.ok(
      tools.some((item) => item.name === "get_hud"),
      "HUD tools stay listed",
    );
    const result = await client.callTool({
      name: "some_tool",
      arguments: { text: "hello" },
      _meta: {
        threadId: "thread-1",
        "x-codex-turn-metadata": { thread_id: "thread-1", turn_id: "turn-9" },
      },
    });
    assert.deepEqual(result.structuredContent, { echoed: "some_tool" });
    const call = fake
      .requests()
      .find((request) => request.method === "host_tool");
    assert.deepEqual(call.params, {
      name: "some_tool",
      arguments: { text: "hello" },
      caller: { client: "codex", sessionId: "thread-1", turnId: "turn-9" },
    });
  } finally {
    await client.close();
  }
});

test("a Claude Code caller comes from the environment the client started the relay with", async () => {
  const fake = engine("claude", [tool]);
  const client = await connect(fake.file, {
    CLAUDE_CODE_SESSION_ID: "session-7",
  });
  try {
    await client.callTool({
      name: "some_tool",
      arguments: { text: "hi", caller: "forged" },
    });
    const call = fake
      .requests()
      .find((request) => request.method === "host_tool");
    assert.deepEqual(call.params.caller, {
      client: "claude",
      sessionId: "session-7",
      turnId: null,
    });
    assert.equal(
      call.params.arguments.caller,
      "forged",
      "an argument named caller stays an argument",
    );
  } finally {
    await client.close();
  }
});

test("an Antigravity caller comes from the conversation_id in the call's _meta", async () => {
  const fake = engine("antigravity", [tool]);
  const client = await connect(fake.file);
  try {
    await client.callTool({
      name: "some_tool",
      arguments: { text: "hello from antigravity" },
      _meta: {
        "antigravity.google/conversation_id": "cascade-3",
        "antigravity.google/agent_name": "main",
      },
    });
    const call = fake
      .requests()
      .find((request) => request.method === "host_tool");
    assert.deepEqual(call.params, {
      name: "some_tool",
      arguments: { text: "hello from antigravity" },
      caller: { client: "antigravity", sessionId: "cascade-3", turnId: null },
    });
  } finally {
    await client.close();
  }
});

test("without a caller the host is told so, with the process that started the relay, and without the app there are no host tools", async () => {
  const fake = engine("nobody", [tool]);
  const client = await connect(fake.file);
  try {
    await client.callTool({ name: "some_tool", arguments: { text: "hi" } });
    // DeepSeek Harness names no caller: the app looks it up by the relay's
    // parent, the Harness host whose Agent HUD plugin noted the call.
    assert.deepEqual(
      fake.requests().find((request) => request.method === "host_tool").params,
      {
        name: "some_tool",
        arguments: { text: "hi" },
        caller: null,
        parent: process.pid,
      },
    );
  } finally {
    await client.close();
  }
  const closed = await connect(engine("closed", null).file);
  try {
    const { tools } = await closed.listTools();
    assert.ok(!tools.some((item) => item.name === "some_tool"));
    assert.ok(tools.some((item) => item.name === "get_hud"));
  } finally {
    await closed.close();
  }
});

test("a group switched on after the relay started is listed, and announced, and leaves when switched off", async () => {
  // Antigravity and DeepSeek Harness keep one relay for all their sessions.
  const state = join(folder, "switch.json");
  writeFileSync(state, "null");
  const fake = engine("switch", null, state);
  const changes = [];
  const client = await connect(
    fake.file,
    { AGENTHUD_NEXT_HOST_TOOLS_MS: "100" },
    {
      listChanged: {
        tools: { onChanged: (error, tools) => changes.push(error ?? tools) },
      },
    },
  );
  const listed = async () =>
    (await client.listTools()).tools.map((item) => item.name);
  const until = async (check) => {
    for (let tries = 0; tries < 100 && !(await check()); tries++)
      await new Promise((done) => setTimeout(done, 50));
    assert.ok(await check());
  };
  try {
    assert.ok(!(await listed()).includes("some_tool"), "off at start");
    writeFileSync(state, JSON.stringify([tool]));
    await until(async () => (await listed()).includes("some_tool"));
    await until(() =>
      changes.some(
        (tools) =>
          Array.isArray(tools) &&
          tools.some((item) => item.name === "some_tool"),
      ),
    );
    // An app that cannot be asked leaves the list as it was.
    writeFileSync(state, "null");
    const asked = fake.requests().length;
    await until(() => fake.requests().length >= asked + 3);
    assert.ok((await listed()).includes("some_tool"));
    writeFileSync(state, JSON.stringify([reader]));
    await until(async () => (await listed()).includes("some_reader"));
    assert.ok(!(await listed()).includes("some_tool"));
    writeFileSync(state, JSON.stringify([]));
    await until(async () => !(await listed()).includes("some_reader"));
    assert.ok((await listed()).includes("get_hud"), "HUD tools stay listed");
  } finally {
    await client.close();
  }
});
