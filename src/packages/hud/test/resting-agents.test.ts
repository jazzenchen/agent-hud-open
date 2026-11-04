import { test } from "node:test";
import assert from "node:assert/strict";
import { layoutMarks, restingAgents } from "../src/resting-agents.ts";

const now = Date.parse("2026-10-21T12:00:00Z");
const hoursAgo = (hours: number) =>
  new Date(now - hours * 3_600_000).toISOString();
const quota = (
  vendor: string,
  usedPercent: number | null,
  status = "normal",
) => ({
  id: `${vendor}:w`,
  vendor,
  name: "w",
  usedPercent,
  resetAt: null,
  status,
  account: null,
});
const session = (source: string, updatedAt: string) => ({
  id: source + updatedAt,
  title: "",
  source,
  model: "",
  cwd: null,
  updatedAt,
  state: "idle",
  tokens: 0,
  cost: null,
  contextPercent: null,
  logPath: null,
  parentId: null,
});
const snapshot = {
  quotas: [quota("claude", 40), quota("codex", null, "stale")],
  billing: [
    {
      id: "d",
      vendor: "deepseek",
      label: "API balance",
      amount: 8.85,
      currency: "CNY",
      notice: null,
    },
  ],
  providers: [
    { id: "kimi", name: "Kimi", enabled: true, detected: true },
    { id: "cursor", name: "Cursor", enabled: true, detected: true },
  ],
  sessions: [
    session("kimi", hoursAgo(30)),
    session("cursor", hoursAgo(5)),
    session("opencode", hoursAgo(1)),
    session("pi", hoursAgo(2)),
  ],
  sourcePreferences: { pi: { live: false } },
} as never;

test("a Dynamic dock shows windows' agents, then recent sessions and detected clients", () => {
  // Kimi's session is older than a day; Pi's live status is off; DeepSeek has only a balance.
  assert.deepEqual(restingAgents(snapshot, "logos", now).order, [
    "claude",
    "codex",
    "opencode",
    "cursor",
    "kimi",
  ]);
});

test("a Simple panel shows windows' agents, then the agents billed by balance alone", () => {
  assert.deepEqual(restingAgents(snapshot, "simple", now).order, [
    "claude",
    "codex",
    "deepseek",
    "kimi",
    "cursor",
  ]);
});

test("the notch glow shows windows with a level, then balances", () => {
  assert.deepEqual(restingAgents(snapshot, "notch", now).order, [
    "claude",
    "deepseek",
    "kimi",
    "cursor",
  ]);
});

for (const mode of ["notch", "logos", "simple"] as const) {
  test(`${mode} keeps detected clients without quotas or sessions, but not disabled or absent clients`, () => {
    const onlyInstalled = {
      quotas: [],
      billing: [],
      sessions: [],
      providers: [
        { id: "codex", name: "Codex", enabled: true, detected: true },
        { id: "chatgpt", name: "ChatGPT", enabled: true, detected: true },
        { id: "claude", name: "Claude", enabled: false, detected: true },
        { id: "cursor", name: "Cursor", enabled: true, detected: false },
      ],
    } as never;
    assert.deepEqual(restingAgents(onlyInstalled, mode, now).order, ["codex"]);
    assert.equal(restingAgents(onlyInstalled, mode, now).expiresAt, Infinity);
  });
}

test("marks follow the agent order set in Settings, also when the HUD has a list of its own", () => {
  // Open #31: Settings' agent order is the panel's block order; a balance block stands for its agent.
  const marks = ["claude", "openai", "deepseek", "cursor"].map((key) => ({
    key,
  }));
  const keys = (layout: Parameters<typeof layoutMarks>[1]) =>
    layoutMarks(marks, layout).map((mark) => mark.key);
  assert.deepEqual(keys({}), ["claude", "openai", "deepseek", "cursor"]);
  assert.deepEqual(keys({ order: ["api:deepseek", "codex", "claude"] }), [
    "deepseek",
    "openai",
    "claude",
    "cursor",
  ]);
  // A list of the HUD's own picks the marks; Settings' order still sorts them.
  assert.deepEqual(
    keys({
      order: ["api:deepseek", "cursor", "codex"],
      hud: { agents: ["Claude Code", "Cursor"] },
    }),
    ["cursor", "claude"],
  );
  assert.deepEqual(keys({ hud: { agents: ["Cursor", "Claude Code"] } }), [
    "claude",
    "cursor",
  ]);
  // One that names no mark here leaves Settings' order in charge.
  assert.deepEqual(keys({ order: ["cursor"], hud: { agents: ["kimi"] } }), [
    "cursor",
    "claude",
    "openai",
    "deepseek",
  ]);
});
