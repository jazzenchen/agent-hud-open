import { test } from "node:test";
import assert from "node:assert/strict";
import type { SessionCall, SessionTurnAgent } from "@agent-hud/protocol";
import type { Theme } from "../src/sessions/SessionTheme.tsx";
import { lanes, offset, tipLine } from "../src/sessions/subagent-lanes.ts";

const THEME = {
  light: false,
  secondary: "#888888",
} as Theme;

const iso = (ms: number) => new Date(ms).toISOString();

function makeCall(
  timestampMs: number,
  overrides: Partial<SessionCall> = {},
): SessionCall {
  return {
    timestamp: iso(timestampMs),
    input: 100,
    output: 50,
    newTokens: 150,
    listCost: 0.01,
    ...overrides,
  };
}

test("offset formats seconds, minutes, and hours compactly", () => {
  assert.equal(offset(0), "0s");
  assert.equal(offset(45_200), "45s");
  assert.equal(offset(120_000), "2m");
  assert.equal(offset(150_000), "2m30s");
  assert.equal(offset(3_900_000), "1h05m");
});

test("lanes orders nested sub-agents depth-first by start time and aggregates tokens and costs", () => {
  const t0 = Date.UTC(2026, 0, 1, 0, 0, 0);
  const calls: SessionCall[] = [
    makeCall(t0 + 1_000, {
      log: "main.jsonl",
      subagent: false,
      modelName: "Claude Opus",
      newTokens: 200,
      listCost: 0.05,
    }),
    makeCall(t0 + 8_000, {
      log: "child-b.jsonl",
      subagent: true,
      modelName: "Claude Haiku",
      newTokens: 300,
      listCost: 0.01,
    }),
    makeCall(t0 + 4_000, {
      log: "child-a.jsonl",
      subagent: true,
      modelName: "Claude Sonnet",
      newTokens: 400,
      listCost: null,
    }),
    makeCall(t0 + 5_000, {
      log: "grandchild-a1.jsonl",
      subagent: true,
      model: "claude-haiku-4-5",
      newTokens: 100,
      listCost: 0.002,
    }),
    makeCall(t0 + 30_000, {
      log: "main.jsonl",
      subagent: false,
      modelName: "Claude Opus",
      newTokens: 250,
      listCost: 0.06,
    }),
  ];

  const agents: SessionTurnAgent[] = [
    {
      log: "child-b.jsonl",
      name: "Second worker",
      kind: "Explore",
      parent: "main.jsonl",
      startedAt: iso(t0 + 7_000),
      lastAt: iso(t0 + 12_000),
      running: false,
    },
    {
      log: "grandchild-a1.jsonl",
      name: "Nested helper",
      kind: null,
      parent: "child-a.jsonl",
      startedAt: null,
      lastAt: iso(t0 + 9_000),
      running: false,
    },
    {
      log: "child-a.jsonl",
      name: "First worker",
      kind: "Plan",
      parent: "main.jsonl",
      startedAt: iso(t0 + 3_000),
      lastAt: iso(t0 + 10_000),
      running: false,
    },
  ];

  const rows = lanes(
    calls,
    agents,
    t0,
    t0 + 40_000,
    t0 + 40_000,
    false,
    THEME,
    "en",
  );

  assert.deepEqual(
    rows.map((row) => ({
      key: row.key,
      depth: row.depth,
      parent: row.parent,
    })),
    [
      { key: "main.jsonl", depth: 0, parent: null },
      { key: "child-a.jsonl", depth: 1, parent: 0 },
      { key: "grandchild-a1.jsonl", depth: 2, parent: 1 },
      { key: "child-b.jsonl", depth: 1, parent: 0 },
    ],
  );

  assert.equal(rows[0].sub, "Claude Opus");
  assert.equal(rows[0].added, 450);
  assert.equal(rows[0].unpriced, false);
  assert.ok(rows[0].cost !== null && Math.abs(rows[0].cost - 0.11) < 1e-9);

  // child-a had only an unpriced call -> cost is null and unpriced is true.
  assert.equal(rows[1].sub, "Plan · Claude Sonnet");
  assert.equal(rows[1].cost, null);
  assert.equal(rows[1].unpriced, true);

  // grandchild-a1 had no startedAt, so it falls back to its first call at +5s.
  assert.equal(rows[2].start, t0 + 5_000);
  assert.equal(rows[2].end, t0 + 9_000);
});

test("lanes distinguishes idle sub-agent waits from standalone long steps including boundary touches and point intervals", () => {
  const t0 = Date.UTC(2026, 0, 1, 0, 0, 0);
  const calls: SessionCall[] = [
    makeCall(t0 + 1_000, { log: "main.jsonl", subagent: false }),
    // +1s -> +20s (19s >= 15s): subagent point-interval at +10s overlaps -> "idle"
    makeCall(t0 + 20_000, { log: "main.jsonl", subagent: false }),
    // +20s -> +40s (20s >= 15s): subagent ended right at +20s and next starts at +40s (boundary touch only) -> "long"
    makeCall(t0 + 40_000, { log: "main.jsonl", subagent: false }),
    // +40s -> +60s (20s >= 15s): subagent [40s, 45s] overlaps -> "idle"
    makeCall(t0 + 60_000, { log: "main.jsonl", subagent: false }),
    makeCall(t0 + 10_000, { log: "point.jsonl", subagent: true }),
  ];

  const agents: SessionTurnAgent[] = [
    {
      log: "point.jsonl",
      name: "Instant check",
      kind: null,
      parent: "main.jsonl",
      startedAt: iso(t0 + 10_000),
      lastAt: iso(t0 + 10_000),
      running: false,
    },
    {
      log: "before-gap.jsonl",
      name: "Touches 20s",
      kind: null,
      parent: "main.jsonl",
      startedAt: iso(t0 + 15_000),
      lastAt: iso(t0 + 20_000),
      running: false,
    },
    {
      log: "after-gap.jsonl",
      name: "Starts at 40s",
      kind: null,
      parent: "main.jsonl",
      startedAt: iso(t0 + 40_000),
      lastAt: iso(t0 + 45_000),
      running: false,
    },
  ];

  const [main] = lanes(
    calls,
    agents,
    t0,
    t0 + 70_000,
    t0 + 70_000,
    false,
    THEME,
    "en",
  );

  assert.deepEqual(main.steps, [
    { from: t0, to: t0 + 1_000, kind: "work" },
    { from: t0 + 1_000, to: t0 + 20_000, kind: "idle" },
    { from: t0 + 20_000, to: t0 + 40_000, kind: "long" },
    { from: t0 + 40_000, to: t0 + 60_000, kind: "idle" },
  ]);
});

test("lanes extends non-live main session end to the latest sub-agent without stack overflow at scale", () => {
  const t0 = Date.UTC(2026, 0, 1, 0, 0, 0);
  const agentCount = 2_000;
  const calls: SessionCall[] = [
    makeCall(t0 + 1_000, { log: "main.jsonl", subagent: false }),
  ];
  const agents: SessionTurnAgent[] = [];

  for (let i = 0; i < agentCount; i += 1) {
    const log = `sub-${i}.jsonl`;
    const at = t0 + 2_000 + i * 10;
    calls.push(
      makeCall(at, {
        log,
        subagent: true,
        tools: ["read_file"],
        newTokens: 1_200,
      }),
    );
    agents.push({
      log,
      name: `Worker ${i}`,
      kind: "Task",
      parent: i % 5 === 0 ? "main.jsonl" : `sub-${i - (i % 5)}.jsonl`,
      startedAt: iso(at),
      lastAt: iso(at + 500),
      running: false,
    });
  }

  const startedAt = performance.now();
  const rows = lanes(
    calls,
    agents,
    t0,
    t0 + 100_000,
    t0 + 100_000,
    false,
    THEME,
    "en",
  );
  const elapsedMs = performance.now() - startedAt;

  assert.equal(rows.length, agentCount + 1);
  assert.equal(rows[0].end, t0 + 2_000 + (agentCount - 1) * 10 + 500);
  assert.ok(
    elapsedMs < 250,
    `expected 2,000 sub-agents to compute in <250ms, took ${elapsedMs.toFixed(1)}ms`,
  );

  // Verify tipLine formatting on call vs idle step vs fallback.
  assert.equal(
    tipLine(rows[1], rows[1].start, t0, rows[1].calls[0], undefined, "en"),
    "+2s · read_file · 1.2k new",
  );
  assert.equal(
    tipLine(rows[0], t0 + 10_000, t0, null, rows[0].steps[1], "en"),
    `+10s · Waiting on sub-agents ${offset(rows[0].end - (t0 + 1_000))}`,
  );
});

test("lanes handles 130,000 sub-agents without Math.max spread stack overflow", () => {
  const t0 = Date.UTC(2026, 0, 1, 0, 0, 0);
  const count = 130_000;
  const agents: SessionTurnAgent[] = Array.from({ length: count }, (_, i) => ({
    log: `a-${i}`,
    name: `A${i}`,
    kind: null,
    parent: null,
    startedAt: iso(t0 + i),
    lastAt: iso(t0 + i + 10),
    running: false,
  }));

  const rows = lanes(
    [],
    agents,
    t0,
    t0 + count + 100,
    t0 + count + 100,
    false,
    THEME,
    "en",
  );
  assert.equal(rows.length, count + 1);
  assert.equal(rows[0].end, t0 + count - 1 + 10);
});
