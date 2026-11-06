import { useMemo, useState } from "react";
import type { StatisticsResponse } from "@agent-hud/protocol";
import { row, type StatsTheme } from "../StatsTheme";
import { dayKey, modelAgents, startOfDay } from "./model";
import { dayFacts, heatmapDays, quartileLevels, type Day } from "./days";
import { DayFactList, heatColor } from "./DayFacts";
import { useWidth } from "./Panel";
import { DayTip } from "./DayTip";
import { CalendarPlaceholder } from "./Placeholder";

/** Every day as a cell, week by week, coloured by its tokens in quartiles of
 * the active days; under it the active days, the average, the busiest day
 * and the current run. Hovering a day splits it by agent and model; a click
 * narrows the page to that day. */
export function DayCalendar({
  data,
  agents,
  selectedDay,
  onSelectDay,
  now,
  readBackTo = null,
  theme,
}: {
  data: StatisticsResponse | null;
  agents: string[] | null;
  selectedDay: string | null;
  onSelectDay: (day: string) => void;
  now: number;
  /** While the history is read: logs written on or before this day are not
   * read yet, so its days may be short. */
  readBackTo?: number | null;
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
          {
            cost,
            sessions: agents ? null : (bucket.sessions ?? null),
            models: bucket.models,
          },
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
  const pending = readBackTo === null ? null : startOfDay(readBackTo);
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
  if (!data)
    return (
      <div ref={ref} style={{ minWidth: 0 }}>
        <CalendarPlaceholder
          columns={count}
          width={width}
          label={label}
          gap={gap}
          theme={theme}
        />
      </div>
    );
  return (
    <>
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
                    opacity={pending !== null && day.start <= pending ? 0.4 : 1}
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
            }}
          >
            <span style={{ marginRight: "auto" }}>
              {since
                ? t(`Recorded since ${since}`, `自 ${since} 起有记录`)
                : ""}
            </span>
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
            <DayTip
              day={hovered.day}
              left={
                hovered.x + cell + 268 > width
                  ? Math.max(0, hovered.x - 268)
                  : hovered.x + cell + 8
              }
              top={Math.max(0, hovered.y - 40)}
              pending={pending !== null && hovered.day.start <= pending}
              bucket={buckets.get(hovered.day.key) ?? null}
              available={data.available}
              agents={agents}
              title={dateText(hovered.day.start)}
              theme={theme}
            />
          )}
        </div>
        <DayFactList facts={facts} dateText={dateText} theme={theme} />
      </div>
    </>
  );
}
