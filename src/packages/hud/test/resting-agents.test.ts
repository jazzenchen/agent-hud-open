import { test } from "node:test";
import assert from "node:assert/strict";
import { layoutMarks, restingAgents } from "../src/resting-agents.ts";

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
} as never;

test("a Simple panel shows windows' agents, then the agents billed by balance alone", () => {
  assert.deepEqual(restingAgents(snapshot, "simple"), [
    "claude",
    "codex",
    "deepseek",
    "kimi",
    "cursor",
  ]);
});

test("the notch glow shows windows with a level, then balances", () => {
  assert.deepEqual(restingAgents(snapshot, "notch"), [
    "claude",
    "deepseek",
    "kimi",
    "cursor",
  ]);
});

for (const mode of ["notch", "simple"] as const) {
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
    assert.deepEqual(restingAgents(onlyInstalled, mode), ["codex"]);
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
