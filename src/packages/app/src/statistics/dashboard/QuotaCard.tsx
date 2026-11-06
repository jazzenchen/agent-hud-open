import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { vendorKey } from "@agent-hud/hud/model";
import type { Quota, StatisticsProvider } from "@agent-hud/protocol";
import { card, row, truncate, type StatsTheme } from "../StatsTheme";
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

/** An agent's card when only its quota is known here: its name, and each
 * quota window by day as the other cards show it. */
export function QuotaCard({
  vendor,
  name,
  rows,
  live,
  pending,
  theme,
  light,
}: {
  vendor: string;
  name: string;
  rows: QuotaRow[];
  live: Map<string, Quota>;
  /** The quota's answer, while a filter change waits for it. */
  pending?: Pending | null;
  theme: StatsTheme;
  light: boolean;
}) {
  const zh = Boolean(theme.zh);
  const look = usePendingLook(pending);
  return (
    <div
      className={`stats-reveal ${look.className}`}
      style={{
        ...card(theme),
        ...look.style,
        display: "grid",
        alignContent: "start",
        gap: 12,
        padding: "14px 16px",
      }}
    >
      <div style={{ ...row, gap: 8 }}>
        <HUDLogo source={vendor} size={18} light={light} />
        <strong style={{ ...truncate, fontSize: 13, fontWeight: 600 }}>
          {name}
        </strong>
      </div>
      <span style={{ fontSize: 11, color: theme.secondary }}>
        {zh
          ? "这台 Mac 上没有它的 Token 记录，只有额度读数"
          : "No tokens recorded on this Mac, only its quota readings"}
      </span>
      <div style={{ display: "grid", gap: 4 }}>
        <span style={{ fontSize: 11, color: theme.secondary }}>
          {zh ? "每天的额度" : "Quota by day"}
        </span>
        <QuotaRows
          rows={rows}
          live={live}
          label={64}
          vendors={null}
          theme={theme}
          light={light}
        />
      </div>
    </div>
  );
}
