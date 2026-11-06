import { moneyShort, percent1 } from "@agent-hud/ui/format";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { compact, type StatsTheme } from "../StatsTheme";
import { cacheHit, change, DAY, modelAgents, usd } from "./model";
import { useTweens } from "../motion";
import { Figure } from "./FigureTile";

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
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const language = zh ? "zh" : "en";
  const previous = compared ? data?.previous : null;
  const versus = (value: number | null) => {
    const text = delta(value);
    if (!text || !compared) return null;
    return zh ? `比${compared} ${text}` : `${text} vs ${compared}`;
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
    days > 1 ? t(`≈ ${text} a day`, `日均 ≈ ${text}`) : null;
  return (
    <div className="stats-figures">
      <Figure
        loading={!data}
        label={t("Tokens", "Token")}
        value={tokens === null ? "—" : compact(Math.round(tokensNow))}
        line={
          tokens === null
            ? null
            : (versus(change(tokens, previous?.selectedTokens ?? null)) ??
              daily(compact(perDay(tokens))))
        }
        series={buckets.map((bucket) => bucket.selectedTokens)}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Worth at API prices", "按 API 价格折合")}
        value={cost === null ? "—" : `${money(costNow)}${unpriced ? "+" : ""}`}
        suffix={others || null}
        hint={t(
          "Every token kind at the vendors' API list prices",
          "全部 Token 种类，按各厂商 API 目录价",
        )}
        line={
          cost === null
            ? null
            : (versus(change(cost, usd(previous?.cost?.amounts) ?? null)) ??
              daily(money(perDay(cost))))
        }
        series={buckets.map((bucket) => bucket.costUSD ?? 0)}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Subscriptions returned", "订阅回本")}
        value={
          plan
            ? `${ratioNow >= 10 ? Math.round(ratioNow) : ratioNow.toFixed(1)}×`
            : "—"
        }
        hint={t(
          "What the subscribed agents' calls were worth at API prices, against the plans' monthly prices for the days shown",
          "订阅 Agent 的调用按 API 价格折合，对比所选天数折算的月费",
        )}
        line={
          plan
            ? t(
                `${money(plan.value)} on ${money(plan.price)} of plans`,
                `${money(plan.price)} 订阅换来 ${money(plan.value)}`,
              )
            : data
              ? agentHasPlan
                ? t("No usage in this range", "这段时间没有用量")
                : t(
                    "No priced plan for this agent",
                    "这个 Agent 没有可定价的订阅",
                  )
              : null
        }
        series={buckets.map((bucket) => {
          if (!monthly) return null;
          const value = Object.entries(bucket.modelCostsUSD ?? {})
            .filter(([model]) => prices.has(agents.get(model) ?? ""))
            .reduce((sum, [, amount]) => sum + amount, 0);
          return value / ((monthly * bucketDays) / 30);
        })}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Sessions", "会话")}
        value={
          sessions === null ? "—" : Math.round(sessionsNow).toLocaleString()
        }
        hint={t(
          "Sessions that used tokens; a sub-agent counts toward its session",
          "用过 Token 的会话；子 Agent 计入它所属的会话",
        )}
        line={
          sessions === null
            ? null
            : (versus(change(sessions, previous?.sessionCount ?? null)) ??
              daily(perDay(sessions).toFixed(1)))
        }
        series={buckets.map((bucket) => bucket.sessions ?? 0)}
        theme={theme}
      />
      <Figure
        loading={!data}
        label={t("Cache hit rate", "缓存命中率")}
        value={hit === null ? "—" : percent1(hitNow * 100)}
        hint={t(
          "Cache reads ÷ (new input + cache writes + cache reads)",
          "缓存读取 ÷（新输入 + 缓存写入 + 缓存读取）",
        )}
        line={
          hit !== null && hitBefore !== null && compared
            ? zh
              ? `比${compared} ${hit >= hitBefore ? "↑" : "↓"} ${Math.abs((hit - hitBefore) * 100).toFixed(1)} 个百分点`
              : `${hit >= hitBefore ? "↑" : "↓"} ${Math.abs((hit - hitBefore) * 100).toFixed(1)} pts vs ${compared}`
            : data
              ? t(
                  `${compact(data.totals.tokens.cacheRead)} read from cache`,
                  `${compact(data.totals.tokens.cacheRead)} 从缓存读取`,
                )
              : null
        }
        series={buckets.map((bucket) => cacheHit(bucket.tokens))}
        theme={theme}
      />
    </div>
  );
}

export const spanDays = (span: { from: number; to: number }) =>
  Math.max(1 / 24, (span.to - span.from) / DAY);
