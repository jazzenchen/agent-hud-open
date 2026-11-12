/** A local calendar day as `YYYY-MM-DD`, the same dates as usage records and the statistics page. */
function dayKey(at: number): string {
  const date = new Date(at);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

export interface PointDay {
  start: number;
  points: number;
}

/** Points on each day of a statistics span. A day with no record is 0. */
export function pointsInSpan(
  days: readonly { date: string; points: number }[],
  first: number,
  count: number,
  addDays: (at: number, days: number) => number,
): PointDay[] {
  const byDate = new Map(days.map((day) => [day.date, day.points]));
  return Array.from({ length: count }, (_, index) => {
    const start = addDays(first, index);
    return { start, points: byDate.get(dayKey(start)) ?? 0 };
  });
}
