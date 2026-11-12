import type { BillingAccount, Quota } from "@agent-hud/protocol";
import type { StatsTheme } from "./StatsTheme";
import { PointsDays } from "./PointsDays";
import { QuotaRows, type QuotaRow } from "./dashboard/QuotaRows";

/** The quota windows along the bottom of an agent card, the same row the
 * other cards use. */
export function DailyQuota({
  rows,
  live,
  perRow,
  theme,
  zh,
}: {
  rows: QuotaRow[];
  live: Map<string, Quota> | undefined;
  perRow: number;
  theme: StatsTheme;
  zh: boolean;
}) {
  if (!rows.length || !live) return null;
  return (
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
          {zh ? "每天的额度" : "Quota by day"}
        </span>
        <QuotaRows
          rows={rows}
          live={live}
          label={perRow >= 3 ? 64 : 84}
          vendors={null}
          theme={theme}
          light={theme.light}
        />
      </div>
    </>
  );
}

/** Points spent each day, for an agent whose balance carries that series. */
export function DailyPoints({
  billing,
  dayFirst,
  dayCount,
  perRow,
  theme,
  zh,
}: {
  billing: BillingAccount[];
  dayFirst: number | undefined;
  dayCount: number | undefined;
  perRow: number;
  theme: StatsTheme;
  zh: boolean;
}) {
  const days =
    billing.find((row) => row.pointsDetail?.days?.length)?.pointsDetail?.days ??
    [];
  if (dayFirst == null || dayCount == null || !days.length) return null;
  return (
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
          {zh ? "每天的积分" : "Points by day"}
        </span>
        <PointsDays
          days={days}
          first={dayFirst}
          count={dayCount}
          label={perRow >= 3 ? 64 : 84}
          theme={theme}
        />
      </div>
    </>
  );
}
