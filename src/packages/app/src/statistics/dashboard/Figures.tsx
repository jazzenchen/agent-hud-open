import { moneyShort, percent1 } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { compact, type StatsTheme } from "../StatsTheme";
import { cacheHit, change, DAY, modelAgents, usd } from "./model";
import { useTweens } from "../motion";
import { Figure } from "./FigureTile";
import type { Pending } from "./pending";

/** "↑ 14%", "↓ 3%", "±0%". */
function delta(value: number | null) {
  if (value === null || !Number.isFinite(value)) return null;
  const rounded = Math.round(value * 100);
  if (rounded === 0) return "±0%";
  return `${rounded > 0 ? "↑" : "↓"} ${Math.abs(rounded)}%`;
}

/** The overview's big figures for the range: tokens, their worth at API
 * prices, what the subscriptions returned, sessions and the cache hit rate,
 * each with its trend. */
export function Figures({
  data,
  plan,
  prices,
  compared,
  days,
  agentHasPlan,
  pending,
  theme,
}: {
  data: StatisticsResponse | null;
  plan: { value: number; price: number; ratio: number } | null;
  /** The plans' monthly prices by agent, for the return's trend. */
  prices: Map<string, number>;
  /** "the 7 days before"; null where a comparison would mislead. */
  compared: string | null;
  days: number;
  agentHasPlan: boolean;
  /** The range's answer, while a filter change waits for it. */
  pending: Pending;
  theme: StatsTheme;
}) {
  const t = useT();
  const { language } = useFormat();
  const previous = compared ? data?.previous : null;
  const versus = (value: number | null) => {
    const text = delta(value);
    if (!text || !compared) return null;
    return t("{{change}} vs {{compared}}", { change: text, compared });
  };
  const buckets = data?.buckets ?? [];
  const agents = data ? modelAgents(data) : new Map<string, string>();
  const bucketDays = (data?.query.bucketMinutes ?? 1440) / 1440;
  const monthly = [...prices.values()].reduce((sum, price) => sum + price, 0);
  const tokens = data?.totals.selectedTokens ?? null;
  const cost = data ? usd(data.totals.cost?.amounts) : null;
  const others = Object.entries(data?.totals.cost?.amounts ?? {})
    .filter(([code, amount]) => code !== "USD" && amount > 0)
    .map(([code, amount]) => `+${moneyShort(amount, code, language)}`)
    .join(" ");
  const sessions = data?.sessionCount ?? null;
  const hit = data ? cacheHit(data.totals.tokens) : null;
  // The figures move to their new values rather than jump.
  // They decrypt into place when they first arrive, then count.
  const [tokensNow, costNow, ratioNow, sessionsNow, hitNow] = useTweens(
    [tokens ?? 0, cost ?? 0, plan?.ratio ?? 0, sessions ?? 0, hit ?? 0],
    undefined,
    Boolean(data),
  );
  const hitBefore = previous ? cacheHit(previous.tokens) : null;
  const perDay = (value: number) => value / Math.max(1, days);
  const unpriced = (data?.totals.cost?.unpricedTokens ?? 0) > 0;
  const money = (value: number) => moneyShort(value, "USD", language);
  const daily = (text: string) =>
    days > 1 ? t("≈ {{amount}} a day", { amount: text }) : null;
  return (
    <div className="stats-figures">
      <Figure
        loading={!data}
        label={t("Tokens")}
        value={tokens === null ? "—" : compact(Math.round(tokensNow))}
        line={
          tokens === null
            ? null
            : (versus(change(tokens, previous?.selectedTokens ?? null)) ??
              daily(compact(perDay(tokens))))
        }
        series={buckets.map((bucket) => bucket.selectedTokens)}
        pending={pending}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Worth at API prices")}
        value={cost === null ? "—" : `${money(costNow)}${unpriced ? "+" : ""}`}
        suffix={others || null}
        hint={t("Every token kind at the vendors' API list prices")}
        line={
          cost === null
            ? null
            : (versus(change(cost, usd(previous?.cost?.amounts) ?? null)) ??
              daily(money(perDay(cost))))
        }
        series={buckets.map((bucket) => bucket.costUSD ?? 0)}
        pending={pending}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Subscriptions returned")}
        value={
          plan
            ? `${ratioNow >= 10 ? Math.round(ratioNow) : ratioNow.toFixed(1)}×`
            : "—"
        }
        hint={t(
          "What the subscribed agents' calls were worth at API prices, against the plans' monthly prices for the days shown",
        )}
        line={
          plan
            ? t("{{value}} on {{price}} of plans", {
                value: money(plan.value),
                price: money(plan.price),
              })
            : data
              ? agentHasPlan
                ? t("No usage in this range")
                : t("No priced plan for this agent")
              : null
        }
        series={buckets.map((bucket) => {
          if (!monthly) return null;
          const value = Object.entries(bucket.modelCostsUSD ?? {})
            .filter(([model]) => prices.has(agents.get(model) ?? ""))
            .reduce((sum, [, amount]) => sum + amount, 0);
          return value / ((monthly * bucketDays) / 30);
        })}
        pending={pending}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Sessions")}
        value={
          sessions === null ? "—" : Math.round(sessionsNow).toLocaleString()
        }
        hint={t(
          "Sessions that used tokens; a sub-agent counts toward its session",
        )}
        line={
          sessions === null
            ? null
            : (versus(change(sessions, previous?.sessionCount ?? null)) ??
              daily(perDay(sessions).toFixed(1)))
        }
        series={buckets.map((bucket) => bucket.sessions ?? 0)}
        pending={pending}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Cache hit rate")}
        value={hit === null ? "—" : percent1(hitNow * 100)}
        hint={t("Cache reads ÷ (new input + cache writes + cache reads)")}
        line={
          hit !== null && hitBefore !== null && compared
            ? t("{{change}} pts vs {{compared}}", {
                change: `${hit >= hitBefore ? "↑" : "↓"} ${Math.abs((hit - hitBefore) * 100).toFixed(1)}`,
                compared,
              })
            : data
              ? t("{{tokens}} read from cache", {
                  tokens: compact(data.totals.tokens.cacheRead),
                })
              : null
        }
        series={buckets.map((bucket) => cacheHit(bucket.tokens))}
        pending={pending}
        theme={theme}
      />
    </div>
  );
}

export const spanDays = (span: { from: number; to: number }) =>
  Math.max(1 / 24, (span.to - span.from) / DAY);
