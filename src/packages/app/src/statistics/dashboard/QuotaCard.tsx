import { Card } from "@agent-hud/ui/components/card";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import { quotaPercentLabel } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import type { Account, Quota, StatisticsProvider } from "@agent-hud/protocol";
import {
  agentAccent,
  card,
  number,
  row,
  truncate,
  type StatsTheme,
} from "../StatsTheme";
import { usePendingLook, type Pending } from "./pending";
import { QuotaRows, type QuotaRow } from "./QuotaRows";

/** The quota windows of agents with no card of their own (no tokens
 * recorded on this Mac), by agent. */
export function quotaOnly(
  rows: QuotaRow[],
  providers: StatisticsProvider[],
): [string, QuotaRow[]][] {
  const carded = new Set(providers.map((provider) => vendorKey(provider.id)));
  const byVendor = new Map<string, QuotaRow[]>();
  for (const entry of rows) {
    const vendor = entry.window.vendor;
    if (carded.has(vendorKey(vendor))) continue;
    byVendor.set(vendor, [...(byVendor.get(vendor) ?? []), entry]);
  }
  return [...byVendor];
}

/** An agent's card when only its quota is known here. Same shell as the
 * token cards: the name, the current windows, then each window by day. */
export function QuotaCard({
  vendor,
  name,
  rows,
  live,
  accounts,
  pending,
  theme,
}: {
  vendor: string;
  name: string;
  rows: QuotaRow[];
  live: Map<string, Quota>;
  accounts?: Account[];
  /** The quota's answer, while a filter change waits for it. */
  pending?: Pending | null;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const look = usePendingLook(pending);
  const display = useFormat().quotaDisplay ?? "consumed";
  const plan =
    accounts?.find(
      (account) => vendorKey(account.provider) === vendorKey(vendor),
    )?.plan ?? null;
  const accent = agentAccent(vendor) ?? theme.secondary;
  const locale = zh ? "zh-CN" : "en-US";
  const figure = (quota: Quota | undefined) => {
    const consumed = quota?.metrics?.consumed;
    if (
      consumed &&
      consumed.capacity != null &&
      Number.isFinite(consumed.used)
    ) {
      const shown =
        display === "remaining"
          ? Math.max(0, consumed.capacity - consumed.used)
          : consumed.used;
      const fmt = (value: number) => Math.round(value).toLocaleString(locale);
      return `${fmt(shown)}/${fmt(consumed.capacity)}`;
    }
    return quotaPercentLabel(quota?.usedPercent, display);
  };
  return (
    <Card
      id={`agent-${vendorKey(vendor)}`}
      className={`statistics-agent-card stats-reveal ${look.className}`}
      style={{
        ...card(theme),
        ...look.style,
        display: "grid",
        alignContent: "start",
        gap: 12,
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
          <HUDLogo source={vendor} size={14} light={theme.light} />
        </span>
        <strong style={{ ...truncate, fontSize: 13, fontWeight: 600 }}>
          {name}
        </strong>
        {plan && (
          <span
            style={{
              marginLeft: "auto",
              padding: "2px 7px",
              borderRadius: 100,
              background: theme.segment,
              color: theme.secondary,
              fontSize: 11,
            }}
          >
            {plan}
          </span>
        )}
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(2,minmax(0,1fr))",
          columnGap: 10,
          rowGap: 6,
        }}
      >
        {rows.map((entry) => {
          const quota = live.get(entry.window.id);
          const label =
            quota?.shortLabel || entry.window.shortLabel || entry.window.name;
          return (
            <div
              key={entry.window.id}
              style={{ ...row, gap: 4, justifyContent: "space-between" }}
            >
              <span
                style={{
                  fontSize: 11,
                  color: theme.secondary,
                  whiteSpace: "nowrap",
                  flexShrink: 0,
                }}
              >
                {label}
              </span>
              <strong style={{ ...number, fontSize: 12, fontWeight: 600 }}>
                {figure(quota)}
              </strong>
            </div>
          );
        })}
      </div>
      <div style={{ background: theme.divider, height: 1 }} />
      <div style={{ display: "grid", gap: 4, minWidth: 0 }}>
        <span style={{ fontSize: 11, color: theme.secondary }}>
          {zh ? "每天的额度" : "Quota by day"}
        </span>
        <QuotaRows
          rows={rows}
          live={live}
          label={64}
          vendors={null}
          theme={theme}
          light={theme.light}
        />
      </div>
    </Card>
  );
}
