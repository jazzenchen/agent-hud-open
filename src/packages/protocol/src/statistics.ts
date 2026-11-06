// The Statistics window's token counts, costs and coverage.
import type { Session } from "./session.ts";

/** Independent additive categories; input excludes cache writes, output excludes reasoning. */
export type StatisticsTokenKind =
  "cacheWrite" | "input" | "reasoning" | "output" | "cacheRead";
export type StatisticsTokens = Record<StatisticsTokenKind, number>;
export interface StatisticsQuery {
  /** Half-open interval [from, to), RFC3339 UTC timestamps. */
  from: string;
  to: string;
  bucketMinutes: 15 | 30 | 60 | 1440;
  tokenKinds: StatisticsTokenKind[];
  /** Omitted means all; an empty array explicitly selects none. */
  providers?: string[];
  /** Provider-qualified model IDs from the available catalog; omitted means all. */
  models?: string[];
  /** IANA calendar time zone; the backend owns DST-aware day boundaries. */
  timeZone: string;
  /** "all" prices every token kind, whichever kinds are counted, as the
   * plan value does; omitted prices the counted kinds. */
  costKinds?: "all" | "selected";
}
export interface StatisticsCost {
  /** List-price estimates by currency. No exchange-rate conversion or subscription charge. */
  amounts: Record<string, number>;
  pricedTokens: number;
  unpricedTokens: number;
  unpricedModels: string[];
  /** From Core: the unpriced models as people read them, "Claude Code · qwen3-coder:30b". */
  unpricedNames?: string[];
}
export interface StatisticsTotal {
  tokens: StatisticsTokens;
  /** Backend count for exactly the selected token kinds. */
  selectedTokens: number;
  cost: StatisticsCost | null;
}
export interface StatisticsBucket {
  timestamp: string;
  tokens: StatisticsTokens;
  selectedTokens: number;
  /** Backend-selected token counts by provider-qualified model ID. */
  models: Record<string, number>;
  /** Sessions that worked in the bucket. */
  sessions?: number;
  /** Its list-price estimate in US dollars, in all and by model; models
   * priced in another currency are left out. */
  costUSD?: number;
  modelCostsUSD?: Record<string, number>;
}
export interface StatisticsModel extends StatisticsTotal {
  id: string;
  provider: string;
  name: string;
}
export interface StatisticsSession extends StatisticsTotal {
  /** Metadata opens the recorded session; its lifetime counters are not the interval values. */
  session: Session;
  /** When it last worked in the range. */
  lastAt?: string | null;
}
export interface StatisticsProvider extends StatisticsTotal {
  id: string;
  name: string;
  models: StatisticsModel[];
  topSessions: StatisticsSession[];
  /** From Core (Open's AgentUsage.cacheReadShare): the share of the list price spent reading context back from the cache, 0…1; null when no model has a price. */
  cacheReadShare?: number | null;
  /** From Core (Open's AgentUsage.recachedCost): what sending context again after the cache lapsed cost at list prices, US dollars; null when none was. */
  recachedCost?: number | null;
  series: number[];
  /** Sessions that worked in the range, and when the client last did. */
  sessionCount?: number;
  lastAt?: string | null;
}
export interface StatisticsCoverage {
  state: "complete" | "partial" | "unavailable";
  partial: boolean;
  observedFrom: string | null;
  observedTo: string | null;
  pendingSources: number;
  unattributedEvents: number;
  unavailableProviders: string[];
  notice: string | null;
}
export interface StatisticsPeriod {
  id: "today" | "7d" | "30d";
  from: string;
  to: string;
  totals: StatisticsTotal;
  models: StatisticsModel[];
  coverage: StatisticsCoverage;
}
export interface StatisticsHeatmap {
  from: string;
  to: string;
  /** Monday-first 7 × 24 local calendar cells; values already use the selected kinds. */
  tokens: number[][];
  intensity: number[][];
  models: Record<string, number>[][];
  coverage: StatisticsCoverage;
}
export interface StatisticsResponse {
  kind: "statistics";
  dataRevision: number;
  generatedAt: string;
  query: StatisticsQuery;
  totals: StatisticsTotal;
  buckets: StatisticsBucket[];
  providers: StatisticsProvider[];
  models: StatisticsModel[];
  available: {
    providers: { id: string; name: string }[];
    models: { id: string; provider: string; name: string }[];
  };
  coverage: StatisticsCoverage;
  recent: StatisticsPeriod[];
  /** The range at least a week long, by weekday and hour. */
  heatmap: StatisticsHeatmap;
  /** Sessions that worked in the range. */
  sessionCount?: number;
  /** The stretch of the same length before the range, for comparing; null
   * where the kept usage does not reach back so far. */
  previous?: (StatisticsTotal & { from: string; sessionCount: number }) | null;
}
