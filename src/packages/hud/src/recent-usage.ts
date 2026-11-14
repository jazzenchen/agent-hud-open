// What an agent with no quota window and no balance shows instead: its last
// 24 hours, as Core reports it on each provider row (`recent`). Kept free of
// the UI so the tests can run it; crates/hud vendors.rs writes the same text.
import type { Snapshot } from "@agent-hud/protocol";
import {
  moneyShort,
  tokensShort,
  translate,
  type Language,
} from "@agent-hud/ui/format";
import { agentArtworkKey as vendorKey } from "./agent-artwork.ts";

/** The last 24 hours of one mark's clients, at list prices. */
export interface RecentUsage {
  /** Every kind, cache reads included, as the token chart counts them. */
  tokens: number;
  /** The calls with a list price, by currency. */
  costs: Record<string, number>;
  /** The tokens of the calls with no list price. */
  unpricedTokens: number;
}

/** What the clients behind mark `key` used in the last 24 hours, summed over their provider rows; null when they used nothing. */
export function recentUsage(
  snapshot: Pick<Snapshot, "providers">,
  key: string,
): RecentUsage | null {
  const sum: RecentUsage = { tokens: 0, costs: {}, unpricedTokens: 0 };
  for (const provider of snapshot.providers) {
    const recent = provider.recent;
    if (!recent || vendorKey(provider.id) !== key) continue;
    sum.tokens += recent.tokens;
    sum.unpricedTokens += recent.unpricedTokens;
    for (const [currency, amount] of Object.entries(recent.costs))
      sum.costs[currency] = (sum.costs[currency] ?? 0) + amount;
  }
  return sum.tokens > 0 ? sum : null;
}

/** The cost a mark shows: US dollars when it has any, else its first currency; "+" when some of it has no list price or is in another currency. Null when nothing has a price. */
export function recentCost(
  recent: RecentUsage,
  language: Language,
): string | null {
  const currencies = Object.keys(recent.costs).sort();
  const shown = currencies.includes("USD") ? "USD" : currencies[0];
  if (shown === undefined) return null;
  const more = recent.unpricedTokens > 0 || currencies.length > 1;
  // Calls to a model on this computer cost nothing.
  if (!more && recent.costs[shown] === 0) return translate(language, "Free");
  return `≈${moneyShort(recent.costs[shown], shown, language)}${more ? "+" : ""}`;
}

/** In a few characters: "≈$1.20" at list prices, else "1.2M" tokens; null when nothing was used. */
export function recentLabel(
  recent: RecentUsage | null,
  language: Language,
): string | null {
  if (!recent) return null;
  return recentCost(recent, language) ?? tokensShort(recent.tokens);
}
