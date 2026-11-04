import { memo, useState } from "react";
import { Card } from "@agent-hud/ui/components/card";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import type { BillingAccount } from "@agent-hud/protocol";
import { card, number, plain, row, type StatsTheme } from "./StatsTheme";
import {
  dateFormat,
  localeDateTime,
  moneyAmount,
  moneyShort,
} from "@agent-hud/ui/format";

export const APIBillingCard = memo(function APIBillingCard({
  account,
  theme,
  highlighted = false,
}: {
  account: BillingAccount;
  theme: StatsTheme;
  highlighted?: boolean;
}) {
  const zh = Boolean(theme.zh);
  const [period, setPeriod] = useState<"day" | "week" | "month">("day");
  const metrics = account.metrics?.periods[period];
  const language = zh ? "zh" : "en";
  const money = (amount: number, currency: string) =>
    moneyAmount(amount, currency, language);
  const points = metrics?.series ?? [];
  const peak = Math.max(0.00001, ...points.map((p) => p.amount));
  const stats = [
    {
      label: account.label || (zh ? "API 余额" : "API balance"),
      value: money(account.amount, account.currency),
    },
    {
      label: zh ? "预估花费" : "Estimated cost",
      value: metrics?.cost
        ? moneyAmount(
            metrics.cost.amount,
            metrics.cost.currency,
            language,
            true,
          )
        : "—",
    },
    {
      label: zh ? "日均消耗" : "Burn / day",
      value: metrics?.burnRate
        ? moneyShort(
            metrics.burnRate.perDay,
            metrics.burnRate.currency,
            language,
          ) + (zh ? "/天" : "/day")
        : "—",
    },
    {
      label: zh ? "预计耗尽" : "Depletion",
      value: metrics?.runsOutAt
        ? dateFormat().format(new Date(metrics.runsOutAt))
        : "—",
    },
  ];
  return (
    <Card
      id={`api-${vendorKey(account.vendor)}`}
      className="statistics-billing-card"
      style={{
        ...card(theme),
        ...(highlighted
          ? {
              border: "1.5px solid #30d158",
              boxShadow: "0 0 0 2px rgba(48, 209, 88, 0.2)",
              transition: "border-color 220ms ease, box-shadow 220ms ease",
            }
          : {
              transition: "border-color 220ms ease, box-shadow 220ms ease",
            }),
      }}
    >
      <div style={{ ...row, justifyContent: "space-between" }}>
        <div style={{ ...row, gap: 7 }}>
          <HUDLogo source={account.vendor} size={14} light={theme.light} />
          <strong style={{ fontSize: 13, fontWeight: 600 }}>
            {account.vendor}
          </strong>
          <span style={{ fontSize: 11, color: theme.secondary }}>
            {account.label || (zh ? "API 账户" : "API account")}
          </span>
        </div>
        <div style={{ ...row, gap: 3 }}>
          {(["day", "week", "month"] as const).map((p) => (
            <button
              key={p}
              type="button"
              aria-pressed={period === p}
              onClick={() => setPeriod(p)}
              style={{
                ...plain,
                fontSize: 10,
                padding: "2px 6px",
                borderRadius: 4,
                color: period === p ? theme.text : theme.secondary,
                background: period === p ? theme.segment : "transparent",
              }}
            >
              {{ day: "24h", week: "7d", month: "30d" }[p]}
            </button>
          ))}
        </div>
      </div>
      {account.notice && (
        <div style={{ fontSize: 11, color: theme.secondary }}>
          {account.notice}
        </div>
      )}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
          gap: 12,
        }}
      >
        {stats.map((item) => (
          <div key={item.label} style={{ display: "grid", gap: 2 }}>
            <span style={{ fontSize: 11, color: theme.secondary }}>
              {item.label}
            </span>
            <strong style={{ fontSize: 15, fontWeight: 600, ...number }}>
              {item.value}
            </strong>
          </div>
        ))}
      </div>
      {points.length > 0 && (
        <div
          aria-label={theme.zh ? "API 花费记录" : "API spending history"}
          style={{
            display: "flex",
            alignItems: "flex-end",
            gap: 2,
            height: 36,
          }}
        >
          {points.map((point, index) => (
            <div
              key={`${point.timestamp}-${index}`}
              title={`${dateFormat(undefined, localeDateTime).format(new Date(point.timestamp))} · ${money(point.amount, point.currency)}`}
              style={{
                flex: 1,
                minWidth: 1,
                height: `${Math.max(4, (point.amount / peak) * 100)}%`,
                background: theme.secondary,
                borderRadius: "2px 2px 0 0",
                opacity: 0.7,
              }}
            />
          ))}
        </div>
      )}
    </Card>
  );
});
