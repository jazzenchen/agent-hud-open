import { useEffect, useState } from "react";
import { DollarSign, Flame, Percent } from "lucide-react";
import type { PanelBlockLayout, Snapshot } from "@agent-hud/protocol";
import { dateFormat, localeDateTime, moneyAmount } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { agentVendorName } from "../agent-artwork";
import {
  balanceColumns,
  balanceMeasures,
  visibleBalances,
  type BalanceMeasure,
} from "../balance-rows";
import { noButtonChrome, palette, tabular, type Logo } from "./theme";

/** An agent's API balances: each account's balance, consumption or cost over
 * 24 hours, 7 or 30 days, as figures or a spending chart. */
export function SourceBalances({
  billing,
  light,
  Logo,
  layout = {},
  onStats,
}: {
  billing: Snapshot["billing"];
  light: boolean;
  Logo: Logo;
  layout?: PanelBlockLayout;
  onStats?: () => void;
}) {
  const colors = palette(useGround(light));
  const services = usePanelServices();
  const clickable = Boolean(services?.selectProvider || onStats);
  const [period, setPeriod] = useState<"day" | "week" | "month">(
    layout.period ?? "day",
  );
  const [selected, setMeasure] = useState<BalanceMeasure>("quota");
  useEffect(() => setPeriod(layout.period ?? "day"), [layout.period]);
  const { language } = useFormat();
  const money = (amount: number, currency: string) =>
    moneyAmount(amount, currency, language);
  return (
    <>
      {visibleBalances(billing, layout).map((account) => {
        const isCredits = account.currency.toLowerCase() === "credits";
        const metrics = account.metrics?.periods[period];
        const points = metrics?.series ?? [],
          peak = Math.max(0.00001, ...points.map((point) => point.amount));
        const columns = balanceColumns(account, period, selected, language);
        return (
          <section
            className="hud-source-balance"
            key={account.id}
            style={{ display: "grid", gap: 8 }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <span
                style={{
                  width: 20,
                  display: "inline-flex",
                  justifyContent: "center",
                }}
              >
                <Logo source={account.vendor} size={14} light={light} />
              </span>
              <strong style={{ fontSize: 13, fontWeight: 700, flex: 1 }}>
                {agentVendorName(account.vendor)}
              </strong>
              {!isCredits && (
                <div
                  role="group"
                  aria-label={language === "zh" ? "余额指标" : "Balance metric"}
                  style={{ display: "flex", gap: 2 }}
                >
                  {balanceMeasures
                    .map((key) => ({
                      key,
                      ...{
                        quota: {
                          Icon: Percent,
                          title: language === "zh" ? "余额" : "Balance",
                        },
                        burn: {
                          Icon: Flame,
                          title:
                            language === "zh"
                              ? "每日消耗"
                              : "Consumption per day",
                        },
                        cost: {
                          Icon: DollarSign,
                          title:
                            language === "zh" ? "估算花费" : "Estimated cost",
                        },
                      }[key],
                    }))
                    .map(({ key, Icon, title }) => (
                      <button
                        key={key}
                        aria-pressed={selected === key}
                        title={title}
                        onClick={() => setMeasure(key)}
                        style={{
                          ...noButtonChrome,
                          color:
                            selected === key ? colors.text : colors.tertiary,
                          background:
                            selected === key ? colors.selected : "transparent",
                          borderRadius: 4,
                          padding: 3,
                        }}
                      >
                        <Icon size={12} />
                      </button>
                    ))}
                </div>
              )}
            </div>
            {account.notice && (
              <div
                style={{
                  fontSize: 11,
                  paddingLeft: 26,
                  color: colors.warning,
                }}
              >
                {account.notice}
              </div>
            )}
            {!isCredits && (
              <div
                role="group"
                aria-label={
                  language === "zh" ? "消耗时段" : "Consumption period"
                }
                style={{ display: "flex", gap: 2, justifyContent: "flex-end" }}
              >
                {(["day", "week", "month"] as const).map((value) => (
                  <button
                    key={value}
                    aria-pressed={period === value}
                    onClick={() => setPeriod(value)}
                    style={{
                      ...noButtonChrome,
                      fontSize: 10,
                      padding: "2px 5px",
                      borderRadius: 4,
                      color: period === value ? colors.text : colors.tertiary,
                      background:
                        period === value ? colors.selected : "transparent",
                    }}
                  >
                    {{ day: "24h", week: "7d", month: "30d" }[value]}
                  </button>
                ))}
              </div>
            )}
            {layout.widget === "chart" ? (
              <div
                aria-label={
                  language === "zh"
                    ? "估算花费记录"
                    : "Estimated spending history"
                }
                title={metrics?.coverage.notice ?? undefined}
                style={{
                  display: "flex",
                  alignItems: "flex-end",
                  gap: 2,
                  height: 42,
                  paddingLeft: 26,
                }}
              >
                {points.length ? (
                  points.map((point, index) => (
                    <div
                      key={point.timestamp + index}
                      title={`${dateFormat(undefined, localeDateTime).format(new Date(point.timestamp))} · ${money(point.amount, point.currency)}`}
                      style={{
                        flex: 1,
                        minWidth: 1,
                        height: `${Math.max(2, (point.amount / peak) * 100)}%`,
                        background: colors.secondary,
                        borderRadius: "2px 2px 0 0",
                      }}
                    />
                  ))
                ) : (
                  <span style={{ fontSize: 11, color: colors.secondary }}>
                    —
                  </span>
                )}
              </div>
            ) : (
              <div
                role={clickable ? "button" : undefined}
                tabIndex={clickable ? 0 : undefined}
                onClick={
                  clickable
                    ? () => {
                        services?.selectProvider?.(account.vendor);
                        onStats?.();
                      }
                    : undefined
                }
                style={{
                  display: "flex",
                  gap: 24,
                  paddingLeft: 26,
                  paddingRight: 6,
                  cursor: clickable ? "pointer" : undefined,
                }}
              >
                {columns.map((column) => (
                  <div
                    key={column.name}
                    title={metrics?.coverage.notice ?? undefined}
                    style={{ flex: 1, display: "grid", gap: 3 }}
                  >
                    <span style={{ fontSize: 11, color: colors.secondary }}>
                      {column.name}
                    </span>
                    <span
                      style={{
                        ...tabular,
                        fontSize: 15,
                        fontWeight: 600,
                        color: colors.text,
                      }}
                    >
                      {column.value}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </>
  );
}
