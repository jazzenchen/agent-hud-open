import { test } from "node:test";
import assert from "node:assert/strict";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  addDays,
  cacheHit,
  dayKey,
  planReturn,
  rangeSpan,
  startOfDay,
  weekStart,
} from "../src/statistics/dashboard/model.ts";
import {
  dayFacts,
  heatmapDays,
  quartileLevels,
  type Day,
} from "../src/statistics/dashboard/days.ts";

const tokens = (input: number, cacheRead = 0) => ({
  input,
  output: 0,
  reasoning: 0,
  cacheWrite: 0,
  cacheRead,
});

/** A response with one bucket a day from `from`, each day's tokens given. */
function response(
  from: number,
  values: number[],
  extra: Partial<StatisticsResponse> = {},
): StatisticsResponse {
  return {
    kind: "statistics",
    dataRevision: 1,
    generatedAt: new Date(from).toISOString(),
    query: {
      from: new Date(from).toISOString(),
      to: new Date(addDays(from, values.length)).toISOString(),
      bucketMinutes: 1440,
      tokenKinds: ["input"],
      timeZone: "UTC",
    },
    totals: { tokens: tokens(0), selectedTokens: 0, cost: null },
    buckets: values.map((value, index) => ({
      timestamp: new Date(addDays(from, index)).toISOString(),
      tokens: tokens(value),
      selectedTokens: value,
      models: (value ? { "claude-model:opus": value } : {}) as Record<
        string,
        number
      >,
      modelCostsUSD: (value
        ? { "claude-model:opus": value / 1000 }
        : {}) as Record<string, number>,
    })),
    providers: [],
    models: [],
    available: {
      providers: [{ id: "claude", name: "Claude Code" }],
      models: [{ id: "claude-model:opus", provider: "claude", name: "Opus" }],
    },
    coverage: {
      state: "complete",
      partial: false,
      observedFrom: null,
      observedTo: null,
      pendingSources: 0,
      unattributedEvents: 0,
      unavailableProviders: [],
      notice: null,
    },
    recent: [],
    heatmap: {
      from: "",
      to: "",
      tokens: [],
      intensity: [],
      models: [],
      coverage: {
        state: "complete",
        partial: false,
        observedFrom: null,
        observedTo: null,
        pendingSources: 0,
        unattributedEvents: 0,
        unavailableProviders: [],
        notice: null,
      },
    },
    ...extra,
  };
}

test("a range of days starts at a local midnight and counts today as its last day", () => {
  const now = new Date(2026, 10, 6, 14, 30).getTime();
  const week = rangeSpan({ id: "7d" }, now);
  assert.equal(week.from, new Date(2026, 9, 31).getTime());
  assert.equal(week.to, now);
  assert.equal(week.bucketMinutes, 1440);
  const today = rangeSpan({ id: "today" }, now);
  assert.equal(today.from, startOfDay(now));
  assert.equal(today.bucketMinutes, 60);
  const day = rangeSpan({ id: "day", day: "2026-11-03" }, now);
  assert.equal(day.from, new Date(2026, 10, 3).getTime());
  assert.equal(day.to, new Date(2026, 10, 4).getTime());
  assert.equal(rangeSpan({ id: "1y" }, now).weekly, true);
});

test("weeks start on Monday", () => {
  // 2026-11-06 is a Friday.
  assert.equal(
    dayKey(weekStart(new Date(2026, 10, 6, 9).getTime())),
    "2026-11-02",
  );
});

test("the cache hit rate is cache reads over all input, and none without input", () => {
  assert.equal(cacheHit(tokens(1, 3)), 0.75);
  assert.equal(cacheHit(tokens(0)), null);
});

test("the plans' return prices the plans for as long as the range", () => {
  const data = response(0, [], {
    providers: [
      {
        id: "claude",
        name: "Claude Code",
        tokens: tokens(0),
        selectedTokens: 0,
        cost: {
          amounts: { USD: 300 },
          pricedTokens: 0,
          unpricedTokens: 0,
          unpricedModels: [],
        },
        models: [],
        topSessions: [],
        series: [],
      },
    ],
  });
  const plan = planReturn(data, new Map([["claude", 200]]), {
    from: 0,
    to: 15 * 86_400_000,
  });
  assert.deepEqual(plan, { value: 300, price: 100, ratio: 3 });
  assert.equal(planReturn(data, new Map(), { from: 0, to: 1 }), null);
});

test("the heatmap shows the asked weeks and marks the days before the record", () => {
  const now = new Date(2026, 10, 6, 12).getTime();
  const first = new Date(2026, 10, 3).getTime();
  const data = response(first, [5, 0, 7, 1]);
  const { weeks } = heatmapDays(data, null, now, 4);
  assert.equal(weeks.length, 4);
  assert.equal(weeks[3][0].key, "2026-11-02");
  assert.equal(weeks[3][0].outside, true, "the day before the record");
  assert.equal(weeks[3][1].value, 5);
  assert.equal(weeks[3][5].outside, true, "tomorrow");
});

test("levels split the active days into quartiles, so one big day does not wash out the rest", () => {
  const level = quartileLevels([0, 1, 2, 3, 4, 1000]);
  assert.equal(level(0), 0);
  assert.equal(level(1), 1);
  assert.equal(level(3), 2);
  assert.equal(level(4), 3);
  assert.equal(level(1000), 4);
});

test("the day facts count active days, the busiest and the runs, today still open", () => {
  const day = (key: string, value: number, outside = false): Day => ({
    key,
    start: Date.parse(`${key}T00:00:00`),
    value,
    outside,
  });
  const facts = dayFacts([
    [
      day("2026-11-01", 0, true),
      day("2026-11-02", 3),
      day("2026-11-03", 9),
      day("2026-11-04", 0),
      day("2026-11-05", 2),
      day("2026-11-06", 4),
      day("2026-11-07", 0),
    ],
  ]);
  assert.equal(facts.activeDays, 4);
  assert.equal(facts.recordedDays, 6);
  assert.equal(facts.busiest?.key, "2026-11-03");
  assert.equal(facts.streak, 2);
  assert.equal(facts.longest, 2);
  assert.equal(facts.average, 4.5);
});
