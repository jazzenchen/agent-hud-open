import { test } from "node:test";
import assert from "node:assert/strict";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import { quotaDays } from "../src/statistics/dashboard/quota-days.ts";

const day = 86_400_000;
const first = new Date(2026, 10, 2).getTime();
const at = (days: number, hours = 0) => first + days * day + hours * 3_600_000;

const window = (
  seconds: number,
  extra: Partial<QuotaHistoryWindow>,
): QuotaHistoryWindow => ({
  id: "w",
  vendor: "claude",
  name: "Weekly",
  windowDurationSeconds: seconds,
  resetAt: null,
  segments: [],
  resets: [],
  ...extra,
});

test("a weekly window's day is what it used that day, and how full it stood", () => {
  const days = quotaDays(
    window(604_800, {
      segments: [
        [
          [at(0, 0), 10],
          [at(0, 12), 30],
          [at(1, 12), 50],
          [at(2, 6), 55],
        ],
      ],
    }),
    first,
    4,
  );
  const round = (value: number | null) =>
    value === null ? null : Math.round(value * 100) / 100;
  // At midnight the line between 30% (noon) and 50% (next noon) stands at 40%.
  assert.equal(days[0].value, 40 - 10);
  assert.equal(days[0].level, 40);
  assert.equal(round(days[1].value), round(50 + 5 * (12 / 18) - 40));
  assert.equal(round(days[2].value), round(55 - (50 + 5 * (12 / 18))));
  assert.equal(days[2].level, 55);
  assert.equal(days[3].value, null, "nothing read");
});

test("a run that opens a cycle counts its first reading as spent since the reset", () => {
  const reset = at(1, 2);
  const days = quotaDays(
    window(604_800, {
      resets: [reset],
      segments: [
        [
          [at(0, 0), 80],
          [at(1, 1), 90],
        ],
        [
          [at(1, 3), 4],
          [at(1, 20), 10],
        ],
      ],
    }),
    first,
    2,
  );
  assert.deepEqual(days[1].resets, [reset]);
  assert.equal(Math.round((days[1].value ?? 0) * 10) / 10, 0.4 + 4 + 6);
  assert.equal(days[1].level, 90, "the fuller of the day's two cycles");
});

test("a five-hour window's day is its highest reading, and the cycles that ran out", () => {
  const days = quotaDays(
    window(18_000, {
      cycles: [
        {
          start: at(0, 1),
          end: at(0, 6),
          used: 40,
          readings: 3,
          current: false,
          partial: false,
        },
        {
          start: at(0, 8),
          end: at(0, 13),
          used: 100,
          readings: 3,
          current: false,
          partial: false,
        },
        {
          start: at(1, 22),
          end: at(2, 3),
          used: 70,
          readings: 2,
          current: false,
          partial: false,
        },
      ],
    }),
    first,
    3,
  );
  assert.equal(days[0].value, 100);
  assert.equal(days[0].ranOut, 1);
  assert.equal(
    days[1].value,
    70,
    "a cycle across midnight counts on both days",
  );
  assert.equal(days[2].value, 70);
  assert.equal(days[2].level, 70);
});
