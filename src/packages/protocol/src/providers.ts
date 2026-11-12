// Agents, their signed-in accounts and their balances.

export interface Provider {
  id: string;
  name: string;
  detected: boolean;
  enabled: boolean;
  notice: string | null;
  sessions: number;
  /** From Core: what the client's calls in the last 24 hours used and cost at list prices, money by currency; null when it made none. */
  recent?: {
    tokens: number;
    costs: Record<string, number>;
    unpricedTokens: number;
  } | null;
  accountStatus?: {
    status: "loading" | "ready" | "error" | "unavailable";
    checkedAt: string | null;
    notice: string | null;
    /** Why a shown failure happened: the agent signed out (its own state),
     * the reading missed an answer, or anything else. */
    failure?: "signedOut" | "transient" | "failed" | null;
    failingSince?: string | null;
    /** The vendor's status page, only while a component this agent depends
     * on is not operational. */
    service?: {
      status:
        | "under_maintenance"
        | "degraded_performance"
        | "partial_outage"
        | "major_outage";
      components: string[];
      incident: string | null;
      page: string | null;
      checkedAt: string;
    } | null;
    configurableFields?: string[];
    configured?: boolean;
    /** A quota only the vendor's site shows, read with a sign-in the user
     * makes in Agent HUD's own window (`web_sign_in`): the provider that reads
     * it, and on which of its sites a session is kept. Never the session. */
    webSignIn?: {
      provider: string;
      sites: { host: string; name: string; signedIn: boolean }[];
    } | null;
  };
  /** Finding an installed/configured client does not imply its data adapter is ported. */
  dataStatus?: "supported" | "account-only" | "detection-only";
  discovery?: {
    installed: boolean;
    configured: boolean;
    usage: boolean;
    /** Non-sensitive evidence categories; never tokens, config contents or personal paths. */
    evidence: string[];
  };
}
export interface Account {
  id: string;
  /** A sampled CLI report, whose freshness follows checkedAt. */
  source?: "antigravity_cli";
  provider: string;
  label: string | null;
  plan: string | null;
  checkedAt: string | null;
  /** The last read failed or the client's last report is no longer fresh. */
  stale: boolean;
  /** From Core: the label, else "Account" and the id's end. */
  name?: string;
  /** From Core: the plan in the provider's tier names ("Ultra"). */
  planLabel?: string | null;
  /** From Core: included in the current account inventory. */
  current?: boolean;
  /** From Core: why the last read failed, from its client; shown in amber. */
  warning?: string | null;
  /** From Core: what the client said otherwise. */
  notice?: string | null;
  /** Usage rows the agent's own service reported, when it has no local token ledger. */
  usageDetail?: {
    records: {
      at: string;
      title: string;
      source: string | null;
      model: string | null;
      percent: number | null;
      amount: number | null;
    }[];
  } | null;
  /** Codex's earned usage resets, from the account's rate limits. */
  resetCredits?: ResetCredits | null;
}
export interface ResetCredits {
  availableCount: number;
  /** Earliest expiry first; a credit without a known expiry has none. */
  credits?: { id: string; expiresAt: string | null }[];
}
export interface BillingPeriod {
  from: string;
  to: string;
  hours: number;
  cost: { currency: string; amount: number } | null;
  burnRate: { currency: string; perDay: number } | null;
  runsOutAt: string | null;
  replyRate: {
    tokensPerSecond: number;
    calls: number;
    tokensOut: number;
    lowerBound?: boolean;
  } | null;
  consumed: { used: number; capacity: null; unit: "tokens" } | null;
  tokensPerHour: number | null;
  series: { timestamp: string; amount: number; currency: string }[];
  coverage: { complete: boolean; notice: string | null };
}
export interface BillingAccount {
  id: string;
  accountId?: string | null;
  vendor: string;
  label: string;
  amount: number;
  currency: string;
  planCredits?: number | null;
  rewardCredits?: number | null;
  /** One agent's points the way TeleAgent's detail draws them: a total, the
   * remaining share, and each grant that expires on its own. */
  pointsDetail?: {
    groups: {
      total: number | null;
      remaining: number | null;
      /** Remaining share of `total`, 0–100, already the bar's width. */
      percent: number | null;
      grant: {
        name: string;
        points: number | null;
        /** Whole days until the grant expires. 0 expires today. */
        expireDays: number | null;
      } | null;
    }[];
    /** Points spent lately, for the consumed measure: a daily pace. */
    spent?: { perDay: number | null; days: number } | null;
    /** Each local day in the recent history, oldest first, including quiet days. */
    days?: { date: string; points: number }[] | null;
  } | null;
  resetAt?: string | null;
  notice: string | null;
  /** From Core (Open's AlertPolicy.balanceLevel): critical at or below zero or while the service refuses the balance, warning at or below its currency's line. */
  level?: "normal" | "warning" | "critical";
  metrics?: {
    periods: Partial<Record<"day" | "week" | "month", BillingPeriod>>;
  };
}
