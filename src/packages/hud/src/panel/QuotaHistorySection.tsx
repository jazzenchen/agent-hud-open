import { useEffect, useState } from "react";
import type {
  PanelLayout,
  Quota,
  QuotaHistoryResponse,
  QuotaHistoryWindow,
  Snapshot,
} from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaPercentLabel } from "@agent-hud/ui/format";
import { usePanelQuery } from "../PanelContext";
import { useGround } from "../ground";
import { palette, tabular, type Logo, type Period } from "./theme";
import {
  AddedSectionHeading,
  PeriodButtons,
  SectionNote,
} from "./AddedSectionParts";
import { historyWindowShown } from "./added-sections";
import { cycleLevel, cycleSpan, quotaGroups } from "./quota-groups";

const ellipsis = {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
} as const;

const days: Record<Period, number> = { "24h": 1, "7d": 7, "30d": 30 };
const HEIGHT = 22;
type Colors = ReturnType<typeof palette>;

/** A share's colour: quiet while it is fine, the HUD's warning and critical
 * colours from 70% and 90%. */
const fill = (used: number, colors: Colors) =>
  ({
    normal: colors.secondary,
    warning: colors.warning,
    critical: colors.critical,
  })[cycleLevel(used)];

/** One window's cycles over `from`–`until`, each as wide as the cycle and as
 * high as the share it used: a five-hour cycle a bar, a week a faint band
 * with a line at its height. The running cycle is lighter, with a dotted
 * line where Core's forecast puts it at its reset. */
function CycleLane({
  window,
  quota,
  from,
  until,
  now,
  colors,
}: {
  window: QuotaHistoryWindow;
  quota: Quota | undefined;
  from: number;
  until: number;
  now: number;
  colors: Colors;
}) {
  const span = Math.max(1, until - from);
  const x = (at: number) =>
    ((Math.min(Math.max(at, from), until) - from) / span) * 100;
  const outlook = quota?.metrics?.outlook;
  const projected =
    outlook?.kind === "exhausts"
      ? (outlook.projectedUsedPercent ?? null)
      : null;
  const long = (window.windowDurationSeconds ?? 0) >= 86_400;
  return (
    <div
      style={{
        position: "relative",
        height: HEIGHT,
        boxShadow: `inset 0 -1px 0 ${colors.divider}`,
      }}
    >
      {(window.cycles ?? []).map((cycle) => {
        const left = x(cycle.start);
        const width = Math.max(0.15, x(cycle.end) - left - (long ? 0.3 : 0.1));
        const height =
          cycle.used > 0 ? Math.max(1.5, (cycle.used / 100) * HEIGHT) : 0;
        const color = fill(cycle.used, colors);
        const ahead =
          cycle.current && projected != null && projected > cycle.used + 1
            ? Math.min(100, projected)
            : null;
        return (
          <div key={cycle.start}>
            <div
              title={`${Math.round(cycle.used)}%`}
              style={{
                position: "absolute",
                bottom: 0,
                left: `${left}%`,
                width: `${width}%`,
                height,
                opacity: cycle.current ? 0.55 : 1,
                borderRadius: long ? 0 : "1.5px 1.5px 0 0",
                background: long
                  ? `linear-gradient(${color}, ${color}) top / 100% 1.5px no-repeat, color-mix(in srgb, ${color} 14%, transparent)`
                  : `color-mix(in srgb, ${color} ${cycle.used >= 70 ? 90 : 55}%, transparent)`,
              }}
            />
            {ahead != null && (
              <div
                style={{
                  position: "absolute",
                  left: `${x(now)}%`,
                  width: `${Math.max(0, x(cycle.end) - x(now))}%`,
                  bottom: (ahead / 100) * HEIGHT,
                  borderTop: `1.5px dotted ${fill(ahead, colors)}`,
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/** The hover panel's quota history: for each agent, a lane per shown window
 * with a bar per cycle over the last day, week or month, as tall as the share
 * that cycle used; the layout's agents and hidden models choose the windows,
 * as the Statistics window's filter does. */
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
      from: new Date(to - days[period] * 86_400_000).toISOString(),
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
  const now = Date.parse(snapshot.generatedAt);
  const { from, until } = cycleSpan(to, days[period]);
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
      {quotaGroups(windows).map((group) => (
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
            <span style={{ ...ellipsis, color: colors.text, fontWeight: 600 }}>
              {names.get(group.vendor) ?? group.vendor}
            </span>
            {group.account && (
              <span style={{ ...ellipsis, color: colors.secondary }}>
                {group.account}
              </span>
            )}
          </div>
          {group.windows.map((window) => {
            const quota = live.get(window.id);
            const used =
              quota?.usedPercent != null && quota.status !== "stale"
                ? quota.usedPercent
                : null;
            return (
              <div
                key={window.id}
                title={window.name}
                style={{
                  display: "grid",
                  gridTemplateColumns: "92px minmax(0, 1fr)",
                  gap: 8,
                  alignItems: "end",
                  fontSize: 11,
                }}
              >
                <span style={{ display: "flex", gap: 4, minWidth: 0 }}>
                  <span style={{ ...ellipsis, color: colors.secondary }}>
                    {window.shortLabel ?? window.name}
                  </span>
                  <span
                    style={{
                      ...tabular,
                      marginLeft: "auto",
                      color:
                        used != null && used >= 70
                          ? fill(used, colors)
                          : colors.text,
                    }}
                  >
                    {used != null ? quotaPercentLabel(used, display) : "—"}
                  </span>
                </span>
                <CycleLane
                  window={window}
                  quota={quota}
                  from={from}
                  until={until}
                  now={now}
                  colors={colors}
                />
              </div>
            );
          })}
        </div>
      ))}
    </section>
  );
}
