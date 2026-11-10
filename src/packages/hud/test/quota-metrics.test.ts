import { test } from "node:test";
import assert from "node:assert/strict";
import { quotaMetric } from "../src/quota-metrics.ts";

const now = Date.parse("2026-10-21T12:00:00Z");
const zh = {
  language: "zh" as const,
  calendar: { timeZone: "UTC", firstWeekday: 2 },
};
const en = {
  language: "en" as const,
  calendar: { timeZone: "UTC", firstWeekday: 1 },
};
const row = (extra: object) =>
  ({
    id: "w",
    vendor: "codex",
    name: "Weekly",
    usedPercent: 40,
    resetAt: "2026-10-21T15:00:00Z",
    status: "normal",
    account: null,
    ...extra,
  }) as never;

test("missing quotas read as -- whether or not the window has been observed", () => {
  assert.deepEqual(
    [
      quotaMetric(row({ usedPercent: null }), "quota", now, en).value,
      quotaMetric(row({ usedPercent: null, resetAt: null }), "quota", now, en)
        .detail,
    ],
    ["--", "—"],
  );
  assert.equal(
    quotaMetric(
      row({ usedPercent: null, observedAt: "2026-10-21T11:00:00Z" }),
      "quota",
      now,
      en,
    ).value,
    "--",
  );
  assert.deepEqual(
    [
      quotaMetric(row({}), "cost", now, zh).value,
      quotaMetric(row({}), "cost", now, zh).detail,
    ],
    ["—", "—"],
  );
});

test("a passed reset waits for its reading, and one inside a minute reads <1m", () => {
  assert.equal(
    quotaMetric(row({ resetAt: "2026-10-21T11:00:00Z" }), "quota", now, zh)
      .detail,
    "等待更新",
  );
  assert.equal(
    quotaMetric(row({ resetAt: "2026-10-21T12:00:30Z" }), "quota", now, en)
      .detail,
    "<1m",
  );
});

test("burn rate: sampling is the detail, a run-out before the reset is its time, replies are counted in words", () => {
  const sampling = row({
    metrics: {
      burnRatePctPerHour: null,
      outlook: { kind: "insufficient-data" },
      sampling: { elapsed: 300, required: 1800, afterReset: false },
    },
  });
  assert.deepEqual(
    [
      quotaMetric(sampling, "burn", now, zh).value,
      quotaMetric(sampling, "burn", now, zh).detail,
    ],
    ["—", "采样中"],
  );
  const runsOut = row({
    metrics: {
      burnRatePctPerHour: 12.5,
      outlook: {
        kind: "exhausts",
        beforeReset: true,
        exhaustsAt: "2026-10-21T14:10:00Z",
      },
    },
  });
  assert.deepEqual(
    [
      quotaMetric(runsOut, "burn", now, en).value,
      quotaMetric(runsOut, "burn", now, en).detail,
    ],
    ["12.5%/h", "Wed 14:10"],
  );
  const replies = row({
    metrics: {
      replyRate: { tokensPerSecond: 42.7, calls: 9, tokensOut: 1200 },
    },
  });
  assert.equal(quotaMetric(replies, "tokens", now, zh).detail, "9 次回复");
  assert.equal(quotaMetric(replies, "tokens", now, en).value, "42/s");
  assert.match(quotaMetric(replies, "tokens", now, en).hint, /own call/);
  // Replies timed only by the gaps between calls give a floor.
  const gaps = row({
    metrics: {
      replyRate: {
        tokensPerSecond: 8.25,
        calls: 4,
        tokensOut: 330,
        lowerBound: true,
      },
    },
  });
  assert.equal(quotaMetric(gaps, "tokens", now, en).value, "≥8.3/s");
  assert.match(quotaMetric(gaps, "tokens", now, en).hint, /at least this/);
});

test("a forecast from the whole window says so, and the budget reads in points", () => {
  const early = row({
    metrics: {
      burnRatePctPerHour: null,
      sampling: { elapsed: 120, required: 900, afterReset: true },
      outlook: {
        kind: "exhausts",
        basis: "window",
        ratePctPerHour: 25,
        beforeReset: true,
        exhaustsAt: "2026-10-21T14:24:00Z",
      },
      budget: { expectedUsedPercent: 28, paceDelta: 12.4, status: "ahead" },
    },
  });
  const burn = quotaMetric(early, "burn", now, en);
  assert.deepEqual(
    [burn.value, burn.detail, burn.available, burn.runsOut],
    ["~25.0%/h", "Wed 14:24", true, true],
  );
  assert.equal(burn.budgetPercent, 28);
  assert.match(burn.hint, /Estimated from the whole window/);
  assert.match(burn.hint, /12 pts ahead of budget/);
  const zhBurn = quotaMetric(early, "burn", now, zh);
  assert.match(zhBurn.hint, /按整个窗口估算/);
  assert.match(zhBurn.hint, /比预算超前 12 个点/);
  const under = row({
    metrics: {
      burnRatePctPerHour: 2,
      outlook: { kind: "exhausts", basis: "recent", beforeReset: false },
      budget: { expectedUsedPercent: 48, paceDelta: -8, status: "behind" },
    },
  });
  assert.match(quotaMetric(under, "burn", now, en).hint, /At the recent pace/);
  assert.match(quotaMetric(under, "quota", now, zh).hint, /比预算少用 8 个点/);
  assert.match(
    quotaMetric(
      row({
        metrics: {
          budget: { expectedUsedPercent: 41, paceDelta: -1, status: "onTrack" },
        },
      }),
      "quota",
      now,
      en,
    ).hint,
    /On budget/,
  );
});

test("remaining quota changes the figure without changing readings, risk or other measures", () => {
  const reading = row({
    usedPercent: 92,
    status: "critical",
    metrics: {
      burnRatePctPerHour: 12.5,
      outlook: {
        kind: "exhausts",
        beforeReset: true,
        exhaustsAt: "2026-10-21T12:40:00Z",
      },
      consumed: { used: 1200, capacity: 3000 },
    },
  });
  const before = JSON.stringify(reading);
  const remaining = { ...en, quotaDisplay: "remaining" as const };
  assert.equal(
    quotaMetric(reading, "quota", now, { ...en, quotaDisplay: "consumed" })
      .value,
    "92%",
  );
  const displayed = quotaMetric(reading, "quota", now, remaining);
  assert.equal(displayed.value, "8%");
  assert.match(displayed.hint, /Quota remaining/);
  assert.deepEqual(
    quotaMetric(reading, "burn", now, remaining),
    quotaMetric(reading, "burn", now, en),
  );
  assert.deepEqual(
    quotaMetric(reading, "consumed", now, remaining),
    quotaMetric(reading, "consumed", now, en),
  );
  assert.equal(JSON.stringify(reading), before);
  assert.equal(
    quotaMetric(row({ usedPercent: null }), "quota", now, remaining).value,
    "--",
  );
  assert.equal(
    quotaMetric(
      row({ usedPercent: null, observedAt: "2026-10-21T11:00:00Z" }),
      "quota",
      now,
      remaining,
    ).value,
    "--",
  );
  const stale = quotaMetric(row({ status: "stale" }), "quota", now, remaining);
  assert.equal(stale.value, "60%");
  assert.match(stale.hint, /stale/);
});
