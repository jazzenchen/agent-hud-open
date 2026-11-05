// Quota windows, their metrics and history, and what a plan is worth.

export interface Quota {
  id: string;
  /** A sampled CLI report, whose freshness follows observedAt. */
  source?: "antigravity_cli";
  /** Stable old-provider identities used by saved account/row visibility. */
  accountId?: string | null;
  legacyWindowId?: string | null;
  vendor: string;
  name: string;
  /** Core's short name for tight places: distinct among the account's
   * windows, else the full name. Shown where the full name does not fit. */
  shortLabel?: string;
  /** Whether this window covers every model, as the account reader reports. */
  allModels?: boolean;
  /** The source's fixed window length; absent/null when it does not report one. */
  windowDurationSeconds?: number | null;
  usedPercent: number | null;
  resetAt: string | null;
  status: "normal" | "warning" | "critical" | "stale";
  account: string | null;
  /** When the window was read; none before its first reading. */
  observedAt?: string | null;
  /** Derived by the runtime for this exact window and its model consumers. */
  metrics?: QuotaMetrics;
}
export type QuotaMeasure = "quota" | "burn" | "tokens" | "consumed" | "cost";
export interface QuotaMetrics {
  /** Percentage points consumed per hour from this cycle's persisted readings. */
  burnRatePctPerHour: number | null;
  tokensPerHour: number | null;
  /** Output tokens / qualifying same-log reply spans, never total tokens / wall time. */
  replyRate: {
    tokensPerSecond: number;
    calls: number;
    tokensOut: number;
    /** No call was timed by its log: spans are gaps between calls, so the rate is at least this. */
    lowerBound?: boolean;
  } | null;
  /** Source tally first; otherwise matching ledger usage within the known cycle. */
  consumed: { used: number; capacity: number | null; unit?: string } | null;
  /** List-price estimate, by currency without conversion; not a subscription charge. */
  cost: { amounts: Record<string, number>; hours: number } | null;
  sampling: { elapsed: number; required: number; afterReset: boolean } | null;
  outlook: {
    kind:
      | "untimed"
      | "exhausted"
      | "no-estimate"
      | "no-usage"
      | "insufficient-data"
      | "exhausts";
    /** What the pace of a forecast or of no usage comes from: this cycle's
     * recent readings, or, until they span enough time, the cycle so far on
     * a straight line (used ÷ time since it started, from 3% of it in). Only
     * a forecast from recent readings raises an alert. */
    basis?: "recent" | "window";
    /** The pace the forecast runs on, percentage points per hour. */
    ratePctPerHour?: number;
    exhaustsAt?: string | null;
    beforeReset?: boolean;
    projectedUsedPercent?: number | null;
  } | null;
  /** Where use stands against a steady pace through the cycle when the
   * metrics were worked out: the share that pace would have used by then,
   * how many points over (+) or under (−) it the use is, and whether that is
   * more than 2 points either way. Null without a known cycle or reading. */
  budget?: {
    expectedUsedPercent: number;
    paceDelta: number;
    status: "ahead" | "behind" | "onTrack";
  } | null;
  /** Specific missing facts, safe for display; absence must not be interpreted as zero. */
  unavailable?: Partial<Record<QuotaMeasure, string>>;
  /** Start of observed coverage; no usage before this timestamp is inferred. */
  observedFrom?: string | null;
}
/** What `quota_history` is asked for: a stretch of the last 30 days,
 * RFC3339 or epoch milliseconds; `to` defaults to now, `from` to a day
 * before it. */
export interface QuotaHistoryQuery {
  from?: string | number;
  to?: string | number;
}
/** One shown quota window's readings over the stretch, from Core. */
export interface QuotaHistoryWindow {
  id: string;
  vendor: string;
  accountId?: string | null;
  account?: string | null;
  name: string;
  /** Distinct among the account's windows, else the full name. */
  shortLabel?: string;
  windowDurationSeconds: number | null;
  resetAt: string | null;
  /** The runs a line joins, oldest first, each of [epoch ms, used %]
   * points: its first reading and the last of every bin. The first run may
   * start just before `from`, at the reading that leads into the stretch.
   * Nothing joins one run to the next: a reset or a gap in the readings
   * lies between them. */
  segments: [number, number][][];
  /** Where a new cycle began in the stretch, epoch ms. */
  resets: number[];
  /** The recorded models whose calls in the stretch spend the window, by
   * their statistics ids (`<source>-model:<model>`). */
  models?: string[];
}
/** A filter choice Core names: an agent, or a recorded model by its statistics id. */
export interface FilterChoice {
  id: string;
  name: string;
}
export interface QuotaHistoryResponse {
  kind: "quota_history";
  generatedAt: string;
  from: string;
  to: string;
  binSeconds: number;
  windows: QuotaHistoryWindow[];
  /** Each window's vendor, once, in the windows' order. */
  agents?: FilterChoice[];
  /** Every model a window names. */
  models?: FilterChoice[];
}
/** A subscription tier with a published monthly price, from the provider's catalog. */
export interface PlanTier {
  id: string;
  name: string;
  /** US dollars a month, billed monthly, before tax. */
  monthlyUSD: number;
  /** The usage it allows as a multiple of the base paid tier, where the vendor states it, for its longest window. */
  multiple: number | null;
  /** The multiple for windows under a day, where the vendor states another (Claude Ultra's five hours). */
  shortMultiple?: number | null;
}
/** One signed-in account of a provider with a plan catalog (`plan_value`), from Core. */
export interface PlanValueAccount {
  id: string;
  provider: string;
  /** The provider's name as the page shows it. */
  providerName?: string;
  /** The provider's models over the stretch as the value counts them, most
   * used first, hidden ones included. */
  models?: (FilterChoice & { tokens: number })[];
  name: string;
  /** The reported plan in words, for a plan the catalog does not price. */
  planLabel: string | null;
  plan: PlanTier | null;
  /** The provider's calls on this Mac over the stretch at API list prices,
   * by currency; calls through a relay or a coding plan are left out. */
  value: {
    /** Where the value's record begins: the stretch's start, or later
     * when the ledger's record begins inside it. */
    from: string;
    tokens: number;
    amounts: Record<string, number>;
    unpricedTokens: number;
    /** Some of the provider's logs are still being read. */
    indexing: boolean;
  } | null;
  /** The US dollar value ÷ the plan's monthly price. */
  ratio: number | null;
  /** The value is a lower bound: logs still read, a record shorter than the
   * stretch, unpriced calls, or calls priced in another currency. */
  partial: boolean;
  /** Why there is no value. */
  unavailable: string | null;
  /** A tier that fits better, only where the quota readings under this plan
   * show it: two of the last four cycles of the longest window ran out
   * (upgrade), or at least two showed their peaks and every window's busiest
   * use would fit a cheaper tier at 80% (downgrade). */
  advice: {
    kind: "upgrade" | "downgrade";
    plan: PlanTier;
    monthlyDeltaUSD: number;
    window: {
      id: string;
      name: string;
      shortLabel?: string | null;
      windowDurationSeconds: number;
    };
    cycles: number;
    ranOut: number;
    /** Upgrade: the longest window's busiest cycle; downgrade: the busiest use of any window. */
    busiestPercent: number;
  } | null;
}
/** What `plan_value` is asked for: the models, by statistics id, whose
 * calls the value leaves out. */
export interface PlanValueQuery {
  hiddenModels?: string[];
}
export interface PlanValueResponse {
  kind: "plan_value";
  generatedAt: string;
  /** When the catalog's prices were checked, YYYY-MM-DD. */
  checked: string;
  from: string;
  to: string;
  accounts: PlanValueAccount[];
}
