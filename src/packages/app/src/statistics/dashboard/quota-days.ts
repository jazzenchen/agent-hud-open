// A quota window's readings, day by day: for a window of a day or longer,
// the share of it each day used; for a shorter one, the highest share any of
// its cycles reached that day. Days are the Mac's local calendar days.
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import { addDays } from "./model.ts";

export interface QuotaDay {
  start: number;
  end: number;
  /** The share of a long window the day used, or a short window's highest
   * reading that day; null where nothing was read. */
  value: number | null;
  /** How full the window stood: a long window's reading at the day's end
   * (or its last that day), a short window's highest. It colours the day. */
  level: number | null;
  /** Cycles that reset during the day, epoch ms. */
  resets: number[];
  /** Short cycles that ran out (95% or more) and touched the day. */
  ranOut: number;
}

/** A window lasting a day or more is read as what each day used of it. */
export const isLong = (window: QuotaHistoryWindow) =>
  (window.windowDurationSeconds ?? 0) >= 86_400;

/** The used share at `at` along a run of readings: the straight line between
 * the readings around it, the first or last reading beyond them. */
function usedAt(points: [number, number][], at: number): number {
  if (at <= points[0][0]) return points[0][1];
  const last = points[points.length - 1];
  if (at >= last[0]) return last[1];
  for (let i = 1; i < points.length; i++) {
    const [t1, u1] = points[i];
    if (at <= t1) {
      const [t0, u0] = points[i - 1];
      return t1 === t0 ? u1 : u0 + ((u1 - u0) * (at - t0)) / (t1 - t0);
    }
  }
  return last[1];
}

/** The window's days from `first` (a local midnight), `count` of them. */
export function quotaDays(
  window: QuotaHistoryWindow,
  first: number,
  count: number,
): QuotaDay[] {
  const days: QuotaDay[] = Array.from({ length: count }, (_, index) => ({
    start: addDays(first, index),
    end: addDays(first, index + 1),
    value: null,
    level: null,
    resets: [],
    ranOut: 0,
  }));
  const touching = (from: number, to: number) =>
    days.filter((day) => from < day.end && to > day.start);
  for (const reset of window.resets)
    for (const day of touching(reset, reset + 1)) day.resets.push(reset);
  if (isLong(window)) {
    for (const run of window.segments) {
      if (!run.length) continue;
      const from = run[0][0];
      const to = run[run.length - 1][0];
      // A run that opens a cycle counts its first reading as spent since the
      // reset; one that continues from before the stretch does not.
      const opens = window.resets.some(
        (reset) => reset <= from && from - reset < 86_400_000,
      );
      for (const day of touching(from, to + 1)) {
        const begin = Math.max(day.start, from);
        const finish = Math.min(day.end, to);
        let spent = Math.max(0, usedAt(run, finish) - usedAt(run, begin));
        if (opens && from >= day.start && from < day.end) spent += run[0][1];
        day.value = (day.value ?? 0) + spent;
        day.level = Math.max(day.level ?? 0, usedAt(run, finish));
      }
    }
  } else {
    for (const cycle of window.cycles ?? [])
      for (const day of touching(cycle.start, cycle.end)) {
        day.value = Math.max(day.value ?? 0, cycle.used);
        if (cycle.used >= 95 && !cycle.current) day.ranOut++;
      }
    for (const run of window.segments)
      for (const [at, used] of run)
        for (const day of touching(at, at + 1))
          day.value = Math.max(day.value ?? 0, used);
  }
  for (const day of days) {
    if (day.value !== null) day.value = Math.min(100, day.value);
    if (!isLong(window)) day.level = day.value;
  }
  return days;
}

/** The most a window's days show, for folding quiet windows away. */
export const busiest = (days: QuotaDay[]) =>
  Math.max(0, ...days.map((day) => day.value ?? 0));
