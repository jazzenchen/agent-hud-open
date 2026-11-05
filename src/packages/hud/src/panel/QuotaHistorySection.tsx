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

const durations: Record<Period, number> = {
  "24h": 86_400_000,
  "7d": 7 * 86_400_000,
  "30d": 30 * 86_400_000,
};
type Colors = ReturnType<typeof palette>;

/** One window's used share over the stretch on a fixed 0–100% axis: its runs
 * as lines with the area under them, and a dashed line where a cycle began. */
function HistoryLine({
  window,
  from,
  to,
  color,
  colors,
}: {
  window: QuotaHistoryWindow;
  from: number;
  to: number;
  color: string;
  colors: Colors;
}) {
  const x = (at: number) =>
    (Math.min(Math.max(at, from), to) - from) / (to - from || 1);
  const point = ([at, used]: [number, number]) =>
    `${(x(at) * 1000).toFixed(1)},${(100 - used).toFixed(1)}`;
  return (
    <svg
      viewBox="0 0 1000 100"
      preserveAspectRatio="none"
      aria-hidden
      style={{ display: "block", width: "100%", height: 22 }}
    >
      <line
        x1={0}
        x2={1000}
        y1={99.5}
        y2={99.5}
        stroke={colors.divider}
        vectorEffect="non-scaling-stroke"
      />
      {window.resets.map((at) => (
        <line
          key={at}
          x1={x(at) * 1000}
          x2={x(at) * 1000}
          y1={0}
          y2={100}
          stroke={colors.divider}
          strokeDasharray="2 2"
          vectorEffect="non-scaling-stroke"
        />
      ))}
      {window.segments
        .filter((run) => run.length > 0)
        .map((run, index) => {
          const line = run.map(point).join(" ");
          const first = x(run[0][0]) * 1000;
          const last = x(run[run.length - 1][0]) * 1000;
          return (
            <g key={index}>
              <polygon
                points={`${first},100 ${line} ${last},100`}
                fill={hexAlpha(color, 0.14)}
              />
              <polyline
                points={line}
                fill="none"
                stroke={color}
                strokeWidth={1.5}
                strokeLinejoin="round"
                vectorEffect="non-scaling-stroke"
              />
            </g>
          );
        })}
    </svg>
  );
}

/** The hover panel's quota history: each shown window's used share over the
 * last day, week or month, with its reading now; the layout's agents and
 * hidden models choose the windows, as the Statistics window's filter does. */
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
  // An account is named where its vendor has more than one.
  const accounts = new Map<string, Set<string>>();
  for (const window of query.data?.windows ?? [])
    accounts.set(
      window.vendor,
      (accounts.get(window.vendor) ?? new Set()).add(window.accountId ?? ""),
    );
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
      {windows.map((window) => {
        const quota = live.get(window.id);
        const level =
          quota?.status === "critical" || quota?.status === "warning"
            ? quota.status
            : null;
        const used =
          quota?.usedPercent != null && quota.status !== "stale"
            ? quotaPercentLabel(quota.usedPercent, quotaDisplay ?? "consumed")
            : "—";
        return (
          <div key={window.id} style={{ display: "grid", gap: 3 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 11,
                minWidth: 0,
              }}
            >
              {Logo && <Logo source={window.vendor} size={12} light={light} />}
              <span
                title={window.name}
                style={{
                  color: colors.text,
                  fontWeight: 600,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {window.shortLabel ?? window.name}
              </span>
              {window.account &&
                (accounts.get(window.vendor)?.size ?? 0) > 1 && (
                  <span
                    style={{
                      color: colors.secondary,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {window.account}
                  </span>
                )}
              <span style={{ flex: 1 }} />
              <span
                style={{
                  ...tabular,
                  color: level ? fill[level] : colors.secondary,
                }}
              >
                {used}
              </span>
            </div>
            <HistoryLine
              window={window}
              from={from}
              to={until}
              color={level ? fill[level] : colors.ok}
              colors={colors}
            />
          </div>
        );
      })}
    </section>
  );
}
