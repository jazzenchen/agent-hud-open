import { useMemo, useState } from "react";
import { moneyShort } from "@agent-hud/ui/format";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  compact,
  number,
  raisedCard,
  row,
  type StatsTheme,
} from "../StatsTheme";
import { dayKey, modelAgents } from "./model";
import { dayFacts, heatmapDays, quartileLevels, type Day } from "./days";
import { DayFactList, heatColor } from "./DayFacts";
import { Panel, useWidth } from "./Panel";

/** Every day as a cell, week by week, coloured by its tokens in quartiles of
 * the active days; beside it the active days, the average, the busiest day
 * and the current run. A click narrows the page to that day. */
export function DailyHeatmap({
  data,
  agents,
  selectedDay,
  onSelectDay,
  now,
  theme,
}: {
  data: StatisticsResponse | null;
  agents: string[] | null;
  selectedDay: string | null;
  onSelectDay: (day: string) => void;
  now: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const language = zh ? "zh-CN" : "en-US";
  const { ref, width } = useWidth();
  const [hovered, setHovered] = useState<{
    day: Day;
    x: number;
    y: number;
  } | null>(null);
  // As many weeks as fill the width at a comfortable size, a year at most.
  const label = 26;
  const gap = 3;
  const count = width
    ? Math.max(12, Math.min(53, Math.floor((width - label) / (15 + gap))))
    : 53;
  const { weeks, first } = useMemo(
    () =>
      data
        ? heatmapDays(data, agents, now, count)
        : { weeks: [] as Day[][], first: null },
    [data, agents, now, count],
  );
  const level = useMemo(
    () => quartileLevels(weeks.flat().map((day) => day.value)),
    [weeks],
  );
  const facts = useMemo(() => dayFacts(weeks), [weeks]);
  // Each day's bucket, for the hover card's cost and sessions.
  const buckets = useMemo(() => {
    const owners = data ? modelAgents(data) : new Map<string, string>();
    return new Map(
      (data?.buckets ?? []).map((bucket) => {
        const cost = agents
          ? Object.entries(bucket.modelCostsUSD ?? {})
              .filter(([model]) => agents.includes(owners.get(model) ?? ""))
              .reduce((sum, [, value]) => sum + value, 0)
          : (bucket.costUSD ?? 0);
        return [
          dayKey(Date.parse(bucket.timestamp)),
          { cost, sessions: agents ? null : (bucket.sessions ?? null) },
        ];
      }),
    );
  }, [data, agents]);
  const cell = weeks.length
    ? Math.max(
        8,
        Math.min(24, Math.floor((width - label) / weeks.length) - gap),
      )
    : 14;
  const step = cell + gap;
  const top = 16;
  const svgWidth = label + weeks.length * step;
  const svgHeight = top + 7 * step;
  const months = weeks.map((week, index) => {
    const firstOfMonth = week.find(
      (day) => new Date(day.start).getDate() === 1,
    );
    if (index === 0 && !firstOfMonth && week[0]) return new Date(week[0].start);
    return firstOfMonth ? new Date(firstOfMonth.start) : null;
  });
  const weekday = (index: number) =>
    zh
      ? ["一", "", "三", "", "五", "", ""][index]
      : ["Mon", "", "Wed", "", "Fri", "", ""][index];
  const since = first
    ? new Date(first).toLocaleDateString(language, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : null;
  const dateText = (start: number) =>
    new Date(start).toLocaleDateString(language, {
      weekday: "short",
      month: "short",
      day: "numeric",
    });
  return (
    <Panel
      area="heatmap"
      theme={theme}
      title={t("Daily usage", "每日用量")}
      meta={
        since
          ? t(
              `Tokens per day, recorded since ${since} · click a day to look at it alone`,
              `每天的 Token，自 ${since} 起有记录 · 点一天单独查看`,
            )
          : t("Tokens per day", "每天的 Token")
      }
    >
      <div
        ref={ref}
        style={{
          display: "grid",
          gap: 14,
          minWidth: 0,
          position: "relative",
        }}
      >
        <div style={{ minWidth: 0, display: "grid", gap: 6 }}>
          <svg
            width={svgWidth}
            height={svgHeight}
            role="img"
            aria-label={t("Tokens per day", "每天的 Token")}
            style={{ display: "block", overflow: "visible" }}
            onMouseLeave={() => setHovered(null)}
          >
            {months.map(
              (month, index) =>
                month && (
                  <text
                    key={`m${index}`}
                    x={label + index * step}
                    y={10}
                    fontSize={10}
                    fill={theme.secondary}
                  >
                    {month.toLocaleDateString(language, { month: "short" })}
                  </text>
                ),
            )}
            {Array.from({ length: 7 }, (_, index) => (
              <text
                key={`d${index}`}
                x={0}
                y={top + index * step + cell - 1}
                fontSize={10}
                fill={theme.secondary}
              >
                {weekday(index)}
              </text>
            ))}
            {weeks.map((week, column) =>
              week.map((day, index) => {
                if (day.start > now) return null;
                const x = label + column * step;
                const y = top + index * step;
                const selected = selectedDay === day.key;
                return (
                  <rect
                    key={day.key}
                    x={x}
                    y={y}
                    width={cell}
                    height={cell}
                    rx={Math.min(3, cell / 4)}
                    fill={
                      day.outside
                        ? theme.light
                          ? "rgba(0,0,0,.03)"
                          : "rgba(255,255,255,.025)"
                        : heatColor(level(day.value), theme)
                    }
                    stroke={selected ? theme.text : "none"}
                    strokeWidth={selected ? 1.5 : 0}
                    style={{ cursor: day.outside ? "default" : "pointer" }}
                    onMouseEnter={() => setHovered({ day, x, y })}
                    onClick={() => !day.outside && onSelectDay(day.key)}
                  />
                );
              }),
            )}
          </svg>
          <div
            style={{
              ...row,
              gap: 4,
              fontSize: 10,
              color: theme.secondary,
              width: Math.max(svgWidth, 160),
              maxWidth: "100%",
              justifyContent: "flex-end",
            }}
          >
            <span>{t("Less", "少")}</span>
            {[0, 1, 2, 3, 4].map((value) => (
              <span
                key={value}
                aria-hidden
                style={{
                  width: 11,
                  height: 11,
                  borderRadius: 2,
                  background: heatColor(value, theme),
                }}
              />
            ))}
            <span>{t("More", "多")}</span>
          </div>
          {hovered && (
            <div
              style={{
                ...raisedCard(theme),
                position: "absolute",
                left: Math.min(hovered.x + cell + 8, Math.max(0, width - 200)),
                top: hovered.y - 4,
                minWidth: 160,
                fontSize: 12,
              }}
            >
              <strong style={{ fontWeight: 600 }}>
                {dateText(hovered.day.start)}
              </strong>
              {hovered.day.outside ? (
                <span style={{ color: theme.secondary }}>
                  {t("Before the record starts", "记录开始之前")}
                </span>
              ) : (
                <>
                  <span style={number}>
                    {compact(hovered.day.value)} {t("tokens", "Token")}
                  </span>
                  {(() => {
                    const bucket = buckets.get(hovered.day.key);
                    if (!bucket) return null;
                    return (
                      <span style={{ color: theme.secondary }}>
                        {moneyShort(bucket.cost, "USD", zh ? "zh" : "en")}
                        {bucket.sessions !== null
                          ? zh
                            ? ` · ${bucket.sessions} 个会话`
                            : ` · ${bucket.sessions} ${bucket.sessions === 1 ? "session" : "sessions"}`
                          : ""}
                      </span>
                    );
                  })()}
                </>
              )}
            </div>
          )}
        </div>
        <DayFactList facts={facts} dateText={dateText} theme={theme} />
      </div>
    </Panel>
  );
}
