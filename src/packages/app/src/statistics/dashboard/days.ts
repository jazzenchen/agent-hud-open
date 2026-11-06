// The daily heatmap's rules: its days week by week, their levels and what
// they say in words. Days are the Mac's local calendar days.
import type { StatisticsResponse } from "@agent-hud/protocol";
import { addDays, dayKey, startOfDay, weekStart } from "./model.ts";

export interface Day {
  key: string;
  start: number;
  value: number;
  /** The day lies before the record starts or after today. */
  outside: boolean;
}

/** The heatmap's days, Monday first, the last `count` weeks (a year at the
 * most), with each day's counted tokens: in all, or the picked agents'. Days
 * before the record starts are marked outside. */
export function heatmapDays(
  data: StatisticsResponse,
  agents: string[] | null,
  now: number,
  count: number,
): { weeks: Day[][]; first: number | null } {
  const values = new Map<string, number>();
  const series = agents
    ? data.providers.filter((provider) => agents.includes(provider.id))
    : null;
  let first: number | null = null;
  data.buckets.forEach((bucket, index) => {
    const start = Date.parse(bucket.timestamp);
    const value = series
      ? series.reduce((sum, provider) => sum + (provider.series[index] ?? 0), 0)
      : bucket.selectedTokens;
    if (bucket.selectedTokens > 0 && first === null) first = start;
    values.set(dayKey(start), value);
  });
  const thisWeek = weekStart(now);
  const from = addDays(thisWeek, -(Math.max(1, Math.min(53, count)) - 1) * 7);
  const today = startOfDay(now);
  const weeks: Day[][] = [];
  for (let week = from; week <= thisWeek; week = addDays(week, 7)) {
    const days: Day[] = [];
    for (let offset = 0; offset < 7; offset++) {
      const start = addDays(week, offset);
      const key = dayKey(start);
      days.push({
        key,
        start,
        value: values.get(key) ?? 0,
        outside: start > today || (first !== null && start < startOfDay(first)),
      });
    }
    weeks.push(days);
  }
  return { weeks, first };
}

/** The level, 0–4, of each value: 0 for none, then the quartiles of the
 * days that have any, so one huge day does not wash the rest out. */
export function quartileLevels(values: number[]): (value: number) => number {
  const sorted = values.filter((value) => value > 0).sort((a, b) => a - b);
  if (!sorted.length) return () => 0;
  const at = (share: number) =>
    sorted[Math.min(sorted.length - 1, Math.floor(share * sorted.length))];
  const cuts = [at(0.25), at(0.5), at(0.75)];
  return (value) =>
    value <= 0 ? 0 : 1 + cuts.filter((cut) => value > cut).length;
}

export interface DayFacts {
  activeDays: number;
  recordedDays: number;
  average: number | null;
  busiest: Day | null;
  streak: number;
  longest: number;
}

/** What the heatmap says in words: active days of the recorded ones, the
 * average active day, the busiest day, and the current and longest runs of
 * active days. */
export function dayFacts(weeks: Day[][]): DayFacts {
  const days = weeks.flat().filter((day) => !day.outside);
  const active = days.filter((day) => day.value > 0);
  let busiest: Day | null = null;
  for (const day of active)
    if (!busiest || day.value > busiest.value) busiest = day;
  let run = 0;
  let longest = 0;
  for (const day of days) {
    run = day.value > 0 ? run + 1 : 0;
    longest = Math.max(longest, run);
  }
  // Today still counts toward the run while it has nothing yet.
  let streak = 0;
  for (let index = days.length - 1; index >= 0; index--) {
    if (days[index].value > 0) streak++;
    else if (index !== days.length - 1) break;
  }
  return {
    activeDays: active.length,
    recordedDays: days.length,
    average: active.length
      ? active.reduce((sum, day) => sum + day.value, 0) / active.length
      : null,
    busiest,
    streak,
    longest,
  };
}
