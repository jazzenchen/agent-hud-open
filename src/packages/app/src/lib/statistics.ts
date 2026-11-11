// Pure rules of the Statistics window's Tokens page, as agent-hud-native-open draws it.
import { dateFormat } from "@agent-hud/ui/format";

const day = 86_400_000;

/** A token chart column: the whole bucket it stands for, in epoch milliseconds. */
export interface TokenPlotColumn {
  start: number;
  end: number;
  /** The selected range cuts the bucket, so its count covers only the part inside the range. */
  partial: boolean;
}

/** The stretch the token chart draws, its columns, and where the selected range lies on it. */
export interface TokenPlot {
  start: number;
  end: number;
  columns: TokenPlotColumn[];
  /** Where the selected range starts and ends on the plot, 0…1 from its left edge. */
  from: number;
  to: number;
}

/**
 * Where the token chart's columns sit (Open's `TokenBarsChart.plotInterval`): the plot runs from the first bucket's
 * start to the last bucket's end, so a bucket the selected range cuts at either end is drawn whole, and its count
 * still covers only the part inside the range. Each bucket ends where the next starts, the last a bucket size later,
 * or at the next local midnight for day buckets. Without buckets the plot is the range itself.
 */
export function tokenPlot(
  buckets: readonly { timestamp: string }[],
  query: { from: string; to: string; bucketMinutes: number; timeZone: string },
): TokenPlot {
  const from = Date.parse(query.from),
    to = Date.parse(query.to);
  const starts = buckets.map((bucket) => Date.parse(bucket.timestamp));
  const columns = starts.map((start, index) => {
    const end =
      index + 1 < starts.length
        ? starts[index + 1]
        : query.bucketMinutes >= 1440
          ? nextDayStart(start, query.timeZone)
          : start + query.bucketMinutes * 60_000;
    return { start, end, partial: start < from || end > to };
  });
  const start = columns.length ? Math.min(columns[0].start, from) : from;
  const end = columns.length
    ? Math.max(columns[columns.length - 1].end, to)
    : to;
  const span = end - start;
  const at = (time: number) => (span > 0 ? (time - start) / span : 0);
  return { start, end, columns, from: at(from), to: span > 0 ? at(to) : 1 };
}

/** The UTC offset of `timeZone` at `at`, in milliseconds. */
function zoneOffset(at: number, timeZone: string): number {
  const parts: Record<string, number> = {};
  for (const part of dateFormat("en-US", {
    timeZone,
    hourCycle: "h23",
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
  }).formatToParts(new Date(at)))
    parts[part.type] = Number(part.value);
  const wall = Date.UTC(
    parts.year,
    parts.month - 1,
    parts.day,
    parts.hour,
    parts.minute,
    parts.second,
  );
  return wall - Math.floor(at / 1000) * 1000;
}

/** The local midnight after `at` in `timeZone`: a calendar day can run 23 or 25 hours at a DST change. */
function nextDayStart(at: number, timeZone: string): number {
  const wall = (Math.floor((at + zoneOffset(at, timeZone)) / day) + 1) * day;
  let next = wall - zoneOffset(at, timeZone);
  next = wall - zoneOffset(next, timeZone);
  return wall - zoneOffset(next, timeZone);
}
