import { test } from "node:test";
import assert from "node:assert/strict";
import {
  contextRuns,
  largestPositions,
  niceScale,
  stackTurn,
  stackTurnBars,
  turnTicks,
} from "../src/lib/session-charts.ts";
import { lastMessageTime, sessionWhere } from "../src/lib/sessions.ts";

test("a chart's scale steps in round numbers up to the first step at or above its peak", () => {
  assert.deepEqual(
    niceScale(48_000),
    { step: 20_000, top: 60_000, ticks: [0, 20_000, 40_000, 60_000] },
    "the iPhone's 60k chart",
  );
  assert.deepEqual(
    niceScale(60_000).ticks,
    [0, 20_000, 40_000, 60_000],
    "a peak on a step stays on it",
  );
  assert.equal(niceScale(7_500).step, 2_500);
  assert.deepEqual(niceScale(1.3).ticks, [0, 0.5, 1, 1.5], "dollars step too");
  assert.deepEqual(niceScale(0.042).ticks, [0, 0.02, 0.04, 0.06]);
  assert.deepEqual(
    niceScale(0),
    { step: 1, top: 1, ticks: [0, 1] },
    "an empty chart still has a scale",
  );
  assert.equal(niceScale(170_000, 2).step, 100_000);
});

test("turns are named at the first, a third, two thirds and the last", () => {
  assert.deepEqual(
    turnTicks(26),
    [0, 8, 16, 25],
    "T1, T9, T17, T26 as on the iPhone",
  );
  assert.deepEqual(turnTicks(3), [0, 1, 2]);
  assert.deepEqual(turnTicks(1), [0]);
  assert.deepEqual(turnTicks(0), []);
});

test("the largest turns are picked largest first, the earlier of equals, never an empty one", () => {
  assert.deepEqual(largestPositions([5, 9, 1, 9, 0, 7], 3), [1, 3, 5]);
  assert.deepEqual(largestPositions([0, 0, 2], 3), [2]);
  assert.deepEqual(largestPositions([], 3), []);
});

test("a row's bars stack the session's own kinds, then its sub-agents', from Core's lists", () => {
  const bars = stackTurnBars({
    count: 2,
    own: { cacheWrite: [1_000, 200], input: [20, 5], output: [50, 40] },
    subagents: { input: [50, 0], reasoning: [3, 0] },
    compacted: [1],
  });
  assert.deepEqual(bars[0], {
    parts: [
      { kind: "cacheWrite", value: 1_000, subagent: false },
      { kind: "input", value: 20, subagent: false },
      { kind: "output", value: 50, subagent: false },
      { kind: "input", value: 50, subagent: true },
      { kind: "reasoning", value: 3, subagent: true },
    ],
    total: 1_123,
    compacted: false,
  });
  assert.equal(bars[1].total, 245);
  assert.equal(bars[1].compacted, true);
  assert.deepEqual(
    stackTurnBars({ count: 1, own: null, subagents: null, compacted: [] }),
    [{ parts: [], total: 0, compacted: false }],
  );
});

test("a turn of the page stacks Core's own part and its sub-agents' part, never cache reads", () => {
  const bar = stackTurn(
    {
      input: 20,
      output: 50,
      cacheRead: 2_000,
      cacheWrite: 1_000,
      reasoning: 0,
      total: 3_070,
    },
    {
      input: 50,
      output: 5,
      cacheRead: 7,
      cacheWrite: 0,
      reasoning: 3,
      total: 65,
    },
  );
  assert.equal(bar.total, 1_128);
  assert.deepEqual(
    bar.parts.map((part) => `${part.kind}${part.subagent ? "*" : ""}`),
    ["cacheWrite", "input", "output", "input*", "reasoning*", "output*"],
  );
});

test("the context line breaks at each compaction and skips turns without a context", () => {
  assert.deepEqual(
    contextRuns([
      { context: 10, compacted: false },
      { context: 20 },
      { context: null },
      { context: 5, compacted: true },
      { context: 9 },
    ]),
    [
      [0, 1],
      [3, 4],
    ],
  );
  assert.deepEqual(
    contextRuns([{ context: 5, compacted: true }]),
    [[0]],
    "a first turn compacted starts the line",
  );
  assert.deepEqual(contextRuns([]), []);
});

test("a row names its model, its project with ~ else its folder, and its client", () => {
  const session = {
    model: "claude-opus-4-5",
    modelName: "Opus 4.5",
    cwd: "/Users/me/work/app/sub",
    project: "~/work/app",
  };
  assert.deepEqual(sessionWhere(session, "Claude Code CLI"), [
    "Opus 4.5",
    "~/work/app",
    "Claude Code CLI",
  ]);
  assert.deepEqual(
    sessionWhere({ ...session, project: null }, "Codex Desktop"),
    ["Opus 4.5", "/Users/me/work/app/sub", "Codex Desktop"],
  );
  assert.deepEqual(
    sessionWhere({ model: "gpt-5", cwd: null }, "Codex CLI"),
    ["gpt-5", "Codex CLI"],
    "no folder: the client alone",
  );
});

test("a session dates from its last message, else its last event", () => {
  assert.equal(
    lastMessageTime({
      lastMessageAt: "2026-10-26T08:00:00Z",
      updatedAt: "2026-10-26T09:30:00Z",
    }),
    "2026-10-26T08:00:00Z",
    "a later event, such as a tool call, does not move it",
  );
  assert.equal(
    lastMessageTime({ lastMessageAt: null, updatedAt: "2026-10-26T09:30:00Z" }),
    "2026-10-26T09:30:00Z",
    "no message time: the last event",
  );
  assert.equal(
    lastMessageTime({ updatedAt: "2026-10-26T09:30:00Z" }),
    "2026-10-26T09:30:00Z",
    "an older Core without the field",
  );
});
