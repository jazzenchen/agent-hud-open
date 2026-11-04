import { memo, useState, type ReactNode } from "react";
import { Card } from "@agent-hud/ui/components/card";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  BillingAccount,
  Session,
  StatisticsPeriod,
  StatisticsProvider,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import {
  card,
  compact,
  costHelp,
  costText,
  Coverage,
  Heading,
  integer,
  kindLabel,
  ModelList,
  number,
  plain,
  row,
  truncate,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { Sparkline } from "./UsageChart";
import {
  dateFormat,
  localeDateTime,
  moneyAmount,
  moneyShort,
} from "@agent-hud/ui/format";

export function agentRows<T>(values: T[]): T[][] {
  const sizes = Array.from({ length: Math.floor(values.length / 3) }, () => 3);
  if (values.length % 3 === 2) sizes.push(2);
  if (values.length % 3 === 1) {
    if (!sizes.length) sizes.push(1);
    else {
      sizes.pop();
      sizes.push(2, 2);
    }
  }
  let offset = 0;
  return sizes.map((size) => {
    const result = values.slice(offset, offset + size);
    offset += size;
    return result;
  });
}

export const ProviderCard = memo(function ProviderCard({
  provider,
  total,
  perRow,
  label,
  theme,
  highlighted = false,
  billing = [],
  onSelectSession,
}: {
  provider: StatisticsProvider;
  total: number;
  perRow: number;
  label: string;
  theme: StatsTheme;
  highlighted?: boolean;
  billing?: BillingAccount[];
  onSelectSession?: (session: Session) => void;
}) {
  const zh = Boolean(theme.zh);
  const animatedTokens = useAnimatedNumber(provider.selectedTokens);
  const shareTarget = total
    ? Math.round((provider.selectedTokens / total) * 100)
    : 0;
  const animatedShare = useAnimatedNumber(shareTarget);
  const providerBilling = billing.filter(
    (b) => vendorKey(b.vendor) === vendorKey(provider.id),
  );
  // Where the price went, from Core (Open's AgentCards.moneyNote): reading
  // context back from the cache, and sending it again after the cache lapsed.
  const moneyNote = [
    provider.cacheReadShare != null
      ? (zh ? "缓存读占费用 " : "Cache reads ") +
        `${Math.round(provider.cacheReadShare * 100)}%` +
        (zh ? "" : " of cost")
      : null,
    provider.recachedCost != null
      ? (zh ? "缓存重写 " : "Cache rewrites ") +
        `≈${moneyAmount(provider.recachedCost, "USD", zh ? "zh" : "en")}`
      : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const accent =
    (
      {
        claude: "#d18b68",
        openai: "#10a37f",
        google: "#8e75ff",
        cursor: "#a4a4a9",
        deepseek: "#5366e9",
        zai: "#3b82f6",
        minimax: "#ec4899",
        kimi: "#6366f1",
        github: "#a855f7",
        qwen: "#615ced",
      } as Record<string, string>
    )[vendorKey(provider.id)] ?? theme.secondary;
  const cells: [string, ReactNode][] = [
    "input",
    "output",
    "reasoning",
    "cacheWrite",
    "cacheRead",
  ].map((kind) => [
    kindLabel(kind as StatisticsTokenKind, zh),
    compact(provider.tokens[kind as StatisticsTokenKind]),
  ]);
  cells.push([
    zh ? "费用" : "Cost",
    <span title={costHelp(provider.cost, zh)}>
      {costText(provider.cost, zh)}
    </span>,
  ]);
  const sessions = (
    <div style={{ display: "grid", gap: 7, flex: 1, minWidth: 0 }}>
      <span style={{ color: theme.secondary, fontSize: 11 }}>
        {zh ? "最高会话" : "Top sessions"}
      </span>
      {provider.topSessions.slice(0, 3).map((entry) => (
        <button
          key={entry.session.id}
          type="button"
          disabled={!onSelectSession}
          onClick={() => onSelectSession?.(entry.session)}
          title={
            zh
              ? `${entry.session.title}\n所选范围内 ${integer(entry.selectedTokens)} Token\n按 API 价格估算 ${costText(entry.cost, zh)}`
              : `${entry.session.title}\n${integer(entry.selectedTokens)} tokens in the selected range\n${costText(entry.cost, zh)} at API prices`
          }
          style={{
            ...plain,
            cursor: onSelectSession ? "pointer" : "default",
            minWidth: 0,
          }}
        >
          <div
            style={{
              ...row,
              justifyContent: "space-between",
              fontSize: 12,
              marginBottom: 3,
            }}
          >
            <span style={truncate}>
              {entry.session.title || (zh ? "未命名会话" : "Untitled session")}
            </span>
            <span style={number}>{compact(entry.selectedTokens)}</span>
          </div>
          <div
            style={{
              height: 2,
              borderRadius: 2,
              overflow: "hidden",
              background: theme.track,
            }}
          >
            <div
              style={{
                width: `${provider.selectedTokens ? Math.min(100, (entry.selectedTokens / provider.selectedTokens) * 100) : 0}%`,
                height: "100%",
                background: accent,
                transition: "width 240ms cubic-bezier(0.22, 1, 0.36, 1)",
              }}
            />
          </div>
        </button>
      ))}
      {!provider.topSessions.length && (
        <span style={{ fontSize: 11, color: theme.secondary }}>
          {zh ? "该范围内无已归属会话" : "No attributed sessions in this range"}
        </span>
      )}
    </div>
  );
  return (
    <Card
      id={`agent-${vendorKey(provider.id)}`}
      className="statistics-agent-card"
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
      <div style={{ ...row, gap: 7 }}>
        <span
          style={{
            width: 26,
            height: 26,
            borderRadius: 6,
            background: `${accent}2e`,
            display: "inline-grid",
            placeItems: "center",
            flexShrink: 0,
          }}
        >
          <HUDLogo source={provider.id} size={14} light={theme.light} />
        </span>
        <strong style={{ ...truncate, fontSize: 13, fontWeight: 600 }}>
          {provider.name}
        </strong>
        <span
          title={
            theme.zh
              ? "在所选时段、智能体、模型和 Token 类型中的占比"
              : "Share of the selected range, agents, models and token kinds"
          }
          style={{
            marginLeft: "auto",
            padding: "2px 7px",
            borderRadius: 100,
            background: theme.segment,
            color: theme.secondary,
            fontSize: 11,
            ...number,
          }}
        >
          {total ? `${animatedShare}%` : "—"}
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 10 }}>
        <div style={{ flexShrink: 0 }}>
          <div
            style={{
              fontSize: 24,
              lineHeight: 1.14,
              fontWeight: 600,
              letterSpacing: "-.45px",
              ...number,
            }}
          >
            {compact(animatedTokens)}
          </div>
          <div style={{ color: theme.secondary, fontSize: 11, marginTop: 2 }}>
            {label}
          </div>
        </div>
        <div
          style={{
            marginLeft: "auto",
            width: `${100 / perRow}%`,
            maxWidth: 330 / perRow,
          }}
        >
          <Sparkline values={provider.series} color={accent} />
        </div>
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: `repeat(${6 / perRow},minmax(0,1fr))`,
          columnGap: perRow >= 3 ? 10 : 14,
          rowGap: 6,
        }}
      >
        {cells.map(([name, value]) => (
          <div
            key={name}
            style={{
              ...row,
              gap: 4,
              justifyContent: "space-between",
            }}
          >
            <span
              style={{
                fontSize: 11,
                color: theme.secondary,
                whiteSpace: "nowrap",
                flexShrink: 0,
              }}
            >
              {name}
            </span>
            <strong
              style={{
                ...number,
                fontSize: perRow >= 3 ? 12 : 13,
                fontWeight: 600,
              }}
            >
              {value}
            </strong>
          </div>
        ))}
      </div>
      {(moneyNote !== "" || providerBilling.length > 0) && (
        <div
          style={{
            ...row,
            justifyContent: "space-between",
            fontSize: 10,
            color: theme.secondary,
            marginTop: -2,
          }}
        >
          <span>{moneyNote}</span>
          {providerBilling.length > 0 && (
            <span style={number}>
              {zh ? "API 余额 " : "API balance "}
              {moneyAmount(
                providerBilling[0].amount,
                providerBilling[0].currency,
                zh ? "zh" : "en",
              )}
            </span>
          )}
        </div>
      )}
      <div
        style={{
          display: "flex",
          flexDirection: perRow === 1 ? "row" : "column",
          gap: perRow === 1 ? 16 : 10,
          minWidth: 0,
        }}
      >
        <ModelList
          models={provider.models}
          total={provider.selectedTokens}
          theme={theme}
        />
        <div
          style={{
            background: theme.divider,
            width: perRow === 1 ? 1 : "100%",
            height: perRow === 1 ? "auto" : 1,
          }}
        />
        {sessions}
      </div>
    </Card>
  );
});

const periodTitles = {
  today: ["Today", "今日"],
  "7d": ["Last 7 days", "近 7 天"],
  "30d": ["Last 30 days", "近 30 天"],
} as const;

function RecentPeriodColumn({
  period,
  index,
  theme,
}: {
  period: StatisticsPeriod;
  index: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const animatedTokens = useAnimatedNumber(period.totals.selectedTokens);
  const promptTotal =
    period.totals.tokens.cacheRead +
    period.totals.tokens.cacheWrite +
    period.totals.tokens.input;
  const cacheHitRate =
    promptTotal > 0
      ? `${Math.round((period.totals.tokens.cacheRead / promptTotal) * 100)}%`
      : "—";
  return (
    <div
      style={{
        padding:
          index === 0 ? "0 14px 0 0" : index === 2 ? "0 0 0 14px" : "0 14px",
        borderLeft: index ? `1px solid ${theme.divider}` : 0,
        minWidth: 0,
      }}
    >
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          fontSize: 12,
          fontWeight: 600,
          marginBottom: 6,
        }}
      >
        <span>{periodTitles[period.id][zh ? 1 : 0]}</span>
        <span style={{ fontSize: 15, ...number }}>
          {period.coverage.state === "unavailable"
            ? "—"
            : compact(animatedTokens)}
        </span>
      </div>
      <div
        title={costHelp(period.totals.cost, zh)}
        style={{ color: theme.secondary, fontSize: 11, marginBottom: 4 }}
      >
        {zh
          ? `按 API 价格估算 ${costText(period.totals.cost, zh)}`
          : `At API prices ${costText(period.totals.cost, zh)}`}
      </div>
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          color: theme.secondary,
          fontSize: 11,
          marginBottom: 6,
        }}
      >
        <span>{zh ? "缓存命中" : "Cache hit"}</span>
        <span style={number}>{cacheHitRate}</span>
      </div>
      <ModelList
        models={period.models}
        total={period.totals.selectedTokens}
        theme={theme}
      />
      <Coverage coverage={period.coverage} theme={theme} />
    </div>
  );
}

export const RecentUsage = memo(function RecentUsage({
  periods,
  label,
  theme,
}: {
  periods: StatisticsPeriod[];
  label: string;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <Card className="statistics-recent-card" style={card(theme)}>
      <Heading theme={theme} trailing={label}>
        {zh ? "近期用量" : "Recent usage"}
      </Heading>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3,minmax(0,1fr))",
        }}
      >
        {periods.map((period, index) => (
          <RecentPeriodColumn
            key={period.id}
            period={period}
            index={index}
            theme={theme}
          />
        ))}
      </div>
    </Card>
  );
});

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
