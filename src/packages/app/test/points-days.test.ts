import { test } from "node:test";
import assert from "node:assert/strict";
import { pointsInSpan } from "../src/statistics/points-days.ts";

test("daily points line up with the statistics page's local days", () => {
  const first = new Date(2026, 10, 10).getTime();
  const addDays = (at: number, days: number) =>
    new Date(new Date(at).getFullYear(), new Date(at).getMonth(), new Date(at).getDate() + days).getTime();
  const shown = pointsInSpan(
    [
      { date: "2026-11-10", points: 4 },
      { date: "2026-11-12", points: 9 },
    ],
    first,
    3,
    addDays,
  );
  assert.deepEqual(
    shown.map((day) => day.points),
    [4, 0, 9],
  );
});
