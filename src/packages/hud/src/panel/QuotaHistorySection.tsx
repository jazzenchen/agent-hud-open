import { useEffect, useState } from "react";
import type {
  PanelLayout,
  QuotaHistoryResponse,
  QuotaHistoryWindow,
  Snapshot,
} from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaPercentLabel } from "@agent-hud/ui/format";
import { usePanelQuery } from "../PanelContext";
import { useGround } from "../ground";
import { hexAlpha } from "../tokens";
import { palette, tabular, type Logo, type Period } from "./theme";
import {
  AddedSectionHeading,
  PeriodButtons,
  SectionNote,
} from "./AddedSectionParts";
import { historyWindowShown } from "./added-sections";
import { quotaGroups, windowColor } from "./quota-groups";

const ellipsis = {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;

const durations: Record<Period, number> = {
  "24h": 86_400_000,
  "7d": 7 * 86_400_000,
  "30d": 30 * 86_400_000,
};
type Colors = ReturnType<typeof palette>;

/** One agent's windows over the stretch on a fixed 0–100% axis, a line each
 * in its colour. A chart of one window fills the area under it and marks its
 * resets with dashed rules; with several, a reset is a tick at the foot. */
function HistoryChart({
  windows,
  lineColors,
  from,
  to,
  colors,
}: {
  windows: QuotaHistoryWindow[];
  lineColors: string[];
  from: number;
  to: number;
  colors: Colors;
}) {
  const single = windows.length === 1;
  const x = (at: number) =>
    ((Math.min(Math.max(at, from), to) - from) / (to - from || 1)) * 1000;
  const point = ([at, used]: [number, number]) =>
    `${x(at).toFixed(1)},${(100 - used).toFixed(1)}`;
  return (
    <svg
      viewBox="0 0 1000 100"
      preserveAspectRatio="none"
      aria-hidden
      style={{ display: "block", width: "100%", height: single ? 22 : 32 }}
    >
      <line
        x1={0}
        x2={1000}
        y1={99.5}
        y2={99.5}
        stroke={colors.divider}
        vectorEffect="non-scaling-stroke"
      />
      {windows.map((window, index) =>
        window.resets.map((at) => (
          <line
            key={`${window.id}-${at}`}
            x1={x(at)}
            x2={x(at)}
            y1={single ? 0 : 88}
            y2={100}
            stroke={single ? colors.divider : lineColors[index]}
            strokeDasharray={single ? "2 2" : undefined}
            vectorEffect="non-scaling-stroke"
          />
        )),
      )}
      {windows.map((window, index) =>
        window.segments
          .filter((run) => run.length > 0)
          .map((run, step) => {
            const line = run.map(point).join(" ");
            return (
              <g key={`${window.id}-${step}`}>
                {single && (
                  <polygon
                    points={`${x(run[0][0])},100 ${line} ${x(run[run.length - 1][0])},100`}
                    fill={hexAlpha(lineColors[index], 0.14)}
                  />
                )}
                <polyline
                  points={line}
                  fill="none"
                  stroke={lineColors[index]}
                  strokeWidth={1.5}
                  strokeLinejoin="round"
                  vectorEffect="non-scaling-stroke"
                />
              </g>
            );
          }),
      )}
    </svg>
  );
}

/** The hover panel's quota history: one chart per agent of its shown
 * windows' used share over the last day, week or month, with each window's
 * reading now; the layout's agents and hidden models choose the windows, as
 * the Statistics window's filter does. */
export function QuotaHistorySection({
  snapshot,
  focusVendor,
  light,
  Logo,
  layout = {},
  active = true,
  bare = false,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo?: Logo;
  layout?: PanelLayout["quotaHistory"];
  active?: boolean;
  bare?: boolean;
}) {
  const { language, quotaDisplay } = useFormat();
  const zh = language === "zh";
  const initial: Period =
    layout.period === "week" ? "7d" : layout.period === "month" ? "30d" : "24h";
  const [period, setPeriod] = useState<Period>(initial);
  useEffect(() => setPeriod(initial), [initial]);
  const colors = palette(useGround(light));
  // Asked again every five minutes, and whenever the usage moves.
  const to =
    Math.ceil(Date.parse(snapshot.generatedAt) / 300_000) * 300_000 || 0;
  const query = usePanelQuery<QuotaHistoryResponse>(
    "quota_history",
    {
      from: new Date(to - durations[period]).toISOString(),
      to: new Date(to).toISOString(),
    },
    snapshot.statisticsRevision,
    active,
  );
  const windows = (query.data?.windows ?? []).filter((window) =>
    historyWindowShown(window, layout, focusVendor),
  );
  const live = new Map(snapshot.quotas.map((quota) => [quota.id, quota]));
  const names = new Map(
    (query.data?.agents ?? []).map((agent) => [agent.id, agent.name]),
  );
  const display = quotaDisplay ?? "consumed";
  const from = query.data
    ? Date.parse(query.data.from)
    : to - durations[period];
  const until = query.data ? Date.parse(query.data.to) : to;
  const fill = { critical: colors.critical, warning: colors.warning };
  return (
    <section
      className="hud-source-quota-history"
      style={{
        display: "grid",
        gridTemplateColumns: "minmax(0, 1fr)",
        gap: 6,
        paddingTop: bare ? 0 : 8,
        boxShadow: bare ? undefined : `inset 0 1px 0 ${colors.divider}`,
      }}
    >
      <AddedSectionHeading
        title={zh ? "额度历史" : "Quota history"}
        colors={colors}
      >
        <PeriodButtons
          period={period}
          setPeriod={setPeriod}
          colors={colors}
          zh={zh}
        />
      </AddedSectionHeading>
      {!query.data && (
        <SectionNote colors={colors}>
          {query.error ?? (zh ? "正在加载额度历史…" : "Loading quota history…")}
        </SectionNote>
      )}
      {query.data && !windows.length && (
        <SectionNote colors={colors}>
          {zh ? "暂无可显示的额度窗口" : "No quota windows to show"}
        </SectionNote>
      )}
      {quotaGroups(windows).map((group) => {
        const lineColors = group.windows.map((_, index) =>
          windowColor(index, light),
        );
        return (
          <div key={group.key} style={{ display: "grid", gap: 3 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 11,
                minWidth: 0,
              }}
            >
              {Logo && <Logo source={group.vendor} size={12} light={light} />}
              <span
                style={{ ...ellipsis, color: colors.text, fontWeight: 600 }}
              >
                {names.get(group.vendor) ?? group.vendor}
              </span>
              {group.account && (
                <span style={{ ...ellipsis, color: colors.secondary }}>
                  {group.account}
                </span>
              )}
            </div>
            <div
              style={{
                display: "flex",
                flexWrap: "wrap",
                gap: "2px 12px",
                fontSize: 11,
              }}
            >
              {group.windows.map((window, index) => {
                const quota = live.get(window.id);
                const level =
                  quota?.status === "critical" || quota?.status === "warning"
                    ? quota.status
                    : null;
                return (
                  <span
                    key={window.id}
                    title={window.name}
                    style={{ display: "flex", alignItems: "center", gap: 4 }}
                  >
                    <span
                      aria-hidden
                      style={{
                        width: 8,
                        height: 3,
                        borderRadius: 2,
                        background: lineColors[index],
                      }}
                    />
                    <span style={{ color: colors.secondary }}>
                      {window.shortLabel ?? window.name}
                    </span>
                    <span
                      style={{
                        ...tabular,
                        color: level ? fill[level] : colors.text,
                      }}
                    >
                      {quota?.usedPercent != null && quota.status !== "stale"
                        ? quotaPercentLabel(quota.usedPercent, display)
                        : "—"}
                    </span>
                  </span>
                );
              })}
            </div>
            <HistoryChart
              windows={group.windows}
              lineColors={lineColors}
              from={from}
              to={until}
              colors={colors}
            />
          </div>
        );
      })}
    </section>
  );
}
