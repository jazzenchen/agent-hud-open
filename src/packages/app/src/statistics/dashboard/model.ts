// Pure rules of the Statistics overview: its ranges, the agents behind each
// model, the big figures, the columns of the trend chart and the daily
// heatmap's days and levels. Days are the Mac's local calendar days, the
// same zone the page sends Core.
import type {
  PlanValueResponse,
  StatisticsResponse,
  StatisticsTokenKind,
  StatisticsTokens,
} from "@agent-hud/protocol";

export const DAY = 86_400_000;

export type RangeId = "today" | "7d" | "30d" | "90d" | "1y";
/** A preset range, or one day picked on the heatmap. */
export type Range = { id: RangeId } | { id: "day"; day: string };
export const rangeIds: RangeId[] = ["today", "7d", "30d", "90d", "1y"];

/** Local midnight of the day `at` falls in. */
export function startOfDay(at: number): number {
  const date = new Date(at);
  return new Date(
    date.getFullYear(),
    date.getMonth(),
    date.getDate(),
  ).getTime();
}

/** Local midnight `days` calendar days from the day `at` falls in. */
export function addDays(at: number, days: number): number {
  const date = new Date(at);
  return new Date(
    date.getFullYear(),
    date.getMonth(),
    date.getDate() + days,
  ).getTime();
}

/** The local day `at` falls in, as YYYY-MM-DD. */
export function dayKey(at: number): string {
  const date = new Date(at);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Local midnight of a YYYY-MM-DD day. */
export function dayStart(key: string): number {
  const [year, month, day] = key.split("-").map(Number);
  return new Date(year, month - 1, day).getTime();
}

/** Local midnight of the Monday of the week `at` falls in. */
export function weekStart(at: number): number {
  const day = new Date(at).getDay();
  return addDays(at, -((day + 6) % 7));
}

export interface Span {
  from: number;
  to: number;
  bucketMinutes: 60 | 1440;
  /** The chart groups the days into weeks. */
  weekly: boolean;
}

/** Where a range starts and ends at `now`, and the columns its chart draws:
 * hours for a day, days up to a quarter, weeks for a year. Ranges of days
 * start at a local midnight, so each column is a whole day. */
export function rangeSpan(range: Range, now: number): Span {
  const today = startOfDay(now);
  if (range.id === "day") {
    const from = dayStart(range.day);
    return {
      from,
      to: Math.min(now, addDays(from, 1)),
      bucketMinutes: 60,
      weekly: false,
    };
  }
  if (range.id === "today")
    return { from: today, to: now, bucketMinutes: 60, weekly: false };
  const days = { "7d": 7, "30d": 30, "90d": 90, "1y": 365 }[range.id];
  return {
    from: addDays(today, 1 - days),
    to: now,
    bucketMinutes: 1440,
    weekly: range.id === "1y",
  };
}

/** The heatmap's stretch: from the Monday 52 weeks before this week's to now. */
export function heatmapSpan(now: number): { from: number; to: number } {
  return { from: addDays(weekStart(now), -52 * 7), to: now };
}

/** The counted kinds of `tokens`, summed. */
export function counted(
  tokens: StatisticsTokens,
  kinds: readonly StatisticsTokenKind[],
): number {
  return kinds.reduce((sum, kind) => sum + (tokens[kind] ?? 0), 0);
}

/** The share of input read back from the cache: cache reads ÷ (new input +
 * cache writes + cache reads); null without input. */
export function cacheHit(tokens: StatisticsTokens): number | null {
  const input = tokens.input + tokens.cacheWrite + tokens.cacheRead;
  return input > 0 ? tokens.cacheRead / input : null;
}

/** The US dollars in a cost, or null when it has none. */
export function usd(
  amounts: Record<string, number> | undefined,
): number | null {
  const value = amounts?.USD;
  return typeof value === "number" ? value : null;
}

/** Each model's agent, from the response's catalog. */
export function modelAgents(data: StatisticsResponse): Map<string, string> {
  return new Map(
    data.available.models.map((model) => [model.id, model.provider]),
  );
}

/** The change from `before` to `now`, as a fraction; null when there is
 * nothing to compare with. */
export function change(now: number, before: number | null | undefined) {
  if (before === null || before === undefined || before <= 0) return null;
  return now / before - 1;
}

/** The plans' monthly prices in US dollars, by agent. */
export function planPrices(
  plans: PlanValueResponse | null,
): Map<string, number> {
  const prices = new Map<string, number>();
  for (const account of plans?.accounts ?? []) {
    const price = account.plan?.monthlyUSD;
    if (!price) continue;
    prices.set(account.provider, (prices.get(account.provider) ?? 0) + price);
  }
  return prices;
}

/** What the subscribed agents' calls in the range were worth at API prices,
 * and their plans' price for as long as the range: the plans' monthly price
 * over 30 days. Null without a priced plan among the agents shown. */
export function planReturn(
  data: StatisticsResponse,
  prices: Map<string, number>,
  span: { from: number; to: number },
): { value: number; price: number; ratio: number } | null {
  let value = 0;
  let monthly = 0;
  for (const provider of data.providers) {
    const price = prices.get(provider.id);
    if (!price) continue;
    monthly += price;
    value += usd(provider.cost?.amounts) ?? 0;
  }
  if (!monthly) return null;
  const price = (monthly * Math.max(span.to - span.from, DAY)) / (30 * DAY);
  return { value, price, ratio: value / price };
}
