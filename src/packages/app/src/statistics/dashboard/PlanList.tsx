import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { listCost, listPrice, moneyShort } from "@agent-hud/ui/format";
import type { BillingAccount, PlanValueResponse } from "@agent-hud/protocol";
import { vendorKey } from "@agent-hud/hud/model";
import { number, row, truncate, type StatsTheme } from "../StatsTheme";
import { adviceText } from "../plan-value-text";
import { Panel } from "./Panel";
import { Tween } from "../motion";
import { Bar } from "./Placeholder";

/** Each signed-in plan: its monthly price, what its calls on this Mac over
 * the last 30 days were worth at API list prices, and the multiple. */
export function PlanList({
  data,
  agents,
  balances,
  names,
  range,
  highlighted,
  theme,
  light,
}: {
  data: PlanValueResponse | null;
  agents: string[] | null;
  /** API balances, beside the plans as what is left to spend. */
  balances: BillingAccount[];
  /** Agents' names by id, for the balances. */
  names: Map<string, string>;
  /** The page's range: each agent's worth in it at API prices, its days,
   * and how to say it ("in the last 7 days"). */
  range: { value: Map<string, number>; days: number; words: string } | null;
  /** The vendor another window asked for, ringed for a moment. */
  highlighted: string | null;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const language = zh ? "zh" : "en";
  const tile = {
    display: "grid",
    alignContent: "start",
    gap: 4,
    minWidth: 0,
    padding: "10px 12px",
    borderRadius: 8,
    border: `1px solid ${theme.divider}`,
  } as const;
  const accounts = (data?.accounts ?? []).filter(
    (account) => !agents || agents.includes(account.provider),
  );
  return (
    <Panel
      area="plans"
      theme={theme}
      title={t("Subscriptions and balances", "订阅与余额")}
      meta={t(
        `Each plan's worth at API list prices ${range?.words ?? "in the last 30 days"}, against its price for those days; what each API balance has left`,
        `每个订阅${range?.words ?? "近 30 天"}按 API 目录价折合，对比同样天数的月费；每个 API 余额还剩多少`,
      )}
    >
      {data && !accounts.length && !balances.length && (
        <span style={{ fontSize: 12, color: theme.secondary }}>
          {t("No signed-in plan for this agent", "这个 Agent 没有登录的订阅")}
        </span>
      )}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill,minmax(210px,1fr))",
          gap: 12,
        }}
      >
        {!data &&
          [0, 1, 2, 3].map((index) => (
            <div key={index} style={{ ...tile, gap: 8 }}>
              <Bar width="60%" height={12} theme={theme} />
              <Bar width="80%" height={9} theme={theme} />
            </div>
          ))}
        {accounts.map((account) => {
          // An agent with one plan is valued over the page's range, its
          // price for as many days; otherwise over the plan's last 30 days.
          const own =
            range &&
            account.plan &&
            accounts.filter((other) => other.provider === account.provider)
              .length === 1
              ? (range.value.get(account.provider) ?? 0)
              : null;
          const ratio =
            own !== null && account.plan
              ? own / ((account.plan.monthlyUSD * range!.days) / 30)
              : account.ratio;
          const worth =
            own !== null
              ? `${moneyShort(own, "USD", language)} ${range!.words}`
              : account.value
                ? listCost(account.value.amounts, language) || "—"
                : null;
          return (
            <div key={account.id} className="stats-reveal" style={tile}>
              <div style={{ ...row, justifyContent: "space-between", gap: 10 }}>
                <span style={{ ...row, gap: 7, minWidth: 0 }}>
                  <HUDLogo source={account.provider} size={15} light={light} />
                  <span style={{ fontSize: 13, fontWeight: 600, flex: "none" }}>
                    {account.plan?.name ??
                      account.planLabel ??
                      account.providerName}
                  </span>
                  <span
                    style={{
                      ...truncate,
                      fontSize: 11,
                      color: theme.secondary,
                    }}
                  >
                    {account.name}
                  </span>
                </span>
                <span style={{ ...number, fontSize: 17, fontWeight: 600 }}>
                  {ratio == null ? (
                    "—"
                  ) : (
                    <Tween
                      value={ratio}
                      format={(value) =>
                        `${own === null && account.partial ? "≥" : ""}${value >= 10 ? Math.round(value) : value.toFixed(1)}×`
                      }
                    />
                  )}
                </span>
              </div>
              <span
                style={{
                  fontVariantNumeric: "tabular-nums",
                  fontSize: 11,
                  color: theme.secondary,
                  overflowWrap: "anywhere",
                }}
              >
                {account.plan
                  ? t(
                      `${listPrice(account.plan.monthlyUSD, "USD", language)}/mo`,
                      `每月 ${listPrice(account.plan.monthlyUSD, "USD", language)}`,
                    )
                  : t("No monthly list price", "没有月付标价")}
                {worth
                  ? t(` · worth ${worth}`, ` · 折合 ${worth}`)
                  : account.unavailable
                    ? ` · ${account.unavailable}`
                    : ""}
              </span>
              {account.advice && (
                <span
                  style={{
                    fontSize: 11,
                    color: theme.secondary,
                    lineHeight: 1.4,
                  }}
                >
                  {adviceText(account.advice, zh)}
                </span>
              )}
            </div>
          );
        })}
        {balances.map((balance) => (
          <div
            key={balance.id}
            id={`api-${vendorKey(balance.vendor)}`}
            style={{
              ...tile,
              boxShadow:
                highlighted === vendorKey(balance.vendor)
                  ? `0 0 0 2px ${theme.secondary}`
                  : "none",
            }}
          >
            <div style={{ ...row, justifyContent: "space-between", gap: 10 }}>
              <span style={{ ...row, gap: 7, minWidth: 0 }}>
                <HUDLogo source={balance.vendor} size={15} light={light} />
                <span style={{ fontSize: 13, fontWeight: 600, flex: "none" }}>
                  {names.get(balance.vendor) ?? balance.label}
                </span>
                <span style={{ fontSize: 11, color: theme.secondary }}>
                  {t("API balance", "API 余额")}
                </span>
              </span>
              <span style={{ ...number, fontSize: 17, fontWeight: 600 }}>
                {moneyShort(balance.amount, balance.currency, language)}
              </span>
            </div>
            <span style={{ fontSize: 11, color: theme.secondary }}>
              {balance.notice ??
                (balance.level === "critical"
                  ? t("Used up", "已用完")
                  : balance.level === "warning"
                    ? t("Running low", "余额不多")
                    : t("Prepaid, spent per call", "预付，按调用扣费"))}
            </span>
          </div>
        ))}
      </div>
      {data && (
        <span style={{ fontSize: 10, color: theme.tertiary }}>
          {t(
            `Monthly list prices checked ${data.checked}. Calls through a relay or a coding plan are left out.`,
            `月付标价核对于 ${data.checked}。经中转或 Coding Plan 的调用不计入。`,
          )}
        </span>
      )}
    </Panel>
  );
}
