import { memo, type ReactNode } from "react";
import { Card } from "@agent-hud/ui/components/card";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import type {
  BillingAccount,
  Quota,
  StatisticsProvider,
  StatisticsTokenKind,
} from "@agent-hud/protocol";
import {
  agentAccent,
  card,
  compact,
  kindLabel,
  number,
  row,
  truncate,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { costHelp, costText } from "./cost-text";
import { ModelList } from "./ModelList";
import { Tween } from "./motion";
import { Sparkline } from "./Sparkline";
import { QuotaRows, type QuotaRow } from "./dashboard/QuotaRows";
import { usePendingLook, type Pending } from "./dashboard/pending";
import { moneyAmount } from "@agent-hud/ui/format";
import { useFormat, useT } from "@agent-hud/ui/format-context";

export const ProviderCard = memo(function ProviderCard({
  provider,
  total,
  perRow,
  label,
  theme,
  highlighted = false,
  billing = [],
  quota,
  pending,
}: {
  provider: StatisticsProvider;
  total: number;
  perRow: number;
  label: string;
  theme: StatsTheme;
  highlighted?: boolean;
  billing?: BillingAccount[];
  /** This agent's quota windows, day by day. */
  quota?: { rows: QuotaRow[]; live: Map<string, Quota> };
  /** The answers it waits for when a filter changes. */
  pending?: (Pending | null)[];
}) {
  const t = useT();
  const { language } = useFormat();
  const look = usePendingLook(...(pending ?? []));
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
      ? t("Cache reads {{share}}% of cost", {
          share: Math.round(provider.cacheReadShare * 100),
        })
      : null,
    provider.recachedCost != null
      ? t("Cache rewrites ≈{{cost}}", {
          cost: moneyAmount(provider.recachedCost, "USD", language),
        })
      : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const balance =
    providerBilling[0] &&
    moneyAmount(
      providerBilling[0].amount,
      providerBilling[0].currency,
      language,
    );
  const accent = agentAccent(provider.id) ?? theme.secondary;
  const cells: [string, ReactNode][] = [
    "input",
    "output",
    "reasoning",
    "cacheWrite",
    "cacheRead",
  ].map((kind) => [
    kindLabel(kind as StatisticsTokenKind, language),
    <Tween
      key={kind}
      value={provider.tokens[kind as StatisticsTokenKind]}
      format={(value) => compact(Math.round(value))}
    />,
  ]);
  cells.push([
    t("Cost"),
    <span title={costHelp(provider.cost, language)}>
      {costText(provider.cost, language)}
    </span>,
  ]);
  return (
    <Card
      id={`agent-${vendorKey(provider.id)}`}
      className={`statistics-agent-card stats-reveal ${look.className}`}
      style={{
        ...card(theme),
        ...look.style,
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
          title={t(
            "Share of the selected range, agents, models and token kinds",
          )}
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
              {providerBilling[0].currency === "credits"
                ? t("Available credits {{amount}}", { amount: balance })
                : t("API balance {{amount}}", { amount: balance })}
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
        {quota && quota.rows.length > 0 && (
          <>
            <div
              style={{
                background: theme.divider,
                width: perRow === 1 ? 1 : "100%",
                height: perRow === 1 ? "auto" : 1,
              }}
            />
            <div style={{ display: "grid", gap: 4, minWidth: 0, flex: 1 }}>
              <span style={{ fontSize: 11, color: theme.secondary }}>
                {t("Quota by day")}
              </span>
              <QuotaRows
                rows={quota.rows}
                live={quota.live}
                label={perRow >= 3 ? 64 : 84}
                vendors={null}
                theme={theme}
                light={theme.light}
              />
            </div>
          </>
        )}
      </div>
    </Card>
  );
});
