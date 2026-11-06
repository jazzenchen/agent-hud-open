import { memo } from "react";
import { Tooltip } from "radix-ui";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { hourModels } from "../lib/statistics";
import type {
  StatisticsHeatmap,
  StatisticsResponse,
} from "@agent-hud/protocol";
import {
  integer,
  modelColor,
  number,
  raisedCard,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { Swatch } from "./StatsParts";
import { Panel } from "./dashboard/Panel";
import { heatColor } from "./dashboard/DayFacts";

function HeatmapRow({
  day,
  index,
  data,
  available,
  color,
  theme,
}: {
  day: string;
  index: number;
  data: StatisticsHeatmap;
  available: StatisticsResponse["available"];
  color: (value: number) => string;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <>
      <span style={{ paddingRight: 7 }}>{day}</span>
      {Array.from({ length: 24 }, (_, hour) => {
        const value = data.tokens[index]?.[hour];
        const breakdown = hourModels(data.models[index]?.[hour]);
        const known =
          data.coverage.state !== "unavailable" && value !== undefined;
        const period = `${day} ${String(hour).padStart(2, "0")}:00–${String(hour + 1).padStart(2, "0")}:00`;
        const label = `${period} · ${
          known
            ? zh
              ? `已记录 ${integer(value)} Token`
              : `${integer(value)} recorded tokens`
            : zh
              ? "不可用"
              : "Unavailable"
        }`;
        return (
          <Tooltip.Root key={hour}>
            <Tooltip.Trigger asChild>
              <div
                tabIndex={0}
                aria-label={label}
                className="statistics-heatmap-cell"
                style={{
                  height: 26,
                  borderRadius: 4,
                  background: known
                    ? color(data.intensity[index]?.[hour] ?? 0)
                    : "transparent",
                  border: known ? 0 : "1px dashed currentColor",
                  opacity: known ? 1 : 0.2,
                  transition: "background-color 220ms ease, opacity 220ms ease",
                }}
              />
            </Tooltip.Trigger>
            {/* Open's HeatmapModelDetails: a raised box above the cell, in the
                page's colours. It is drawn outside the page (a portal), so it
                names its own fill and ink rather than inheriting them. */}
            <Tooltip.Portal>
              <Tooltip.Content
                side="top"
                sideOffset={8}
                collisionPadding={8}
                className="statistics-heatmap-details animate-in fade-in-0"
                style={{
                  ...raisedCard(theme),
                  minWidth: 180,
                  maxWidth: 300,
                  fontSize: 11,
                }}
              >
                <div style={{ color: theme.secondary, fontSize: 10 }}>
                  {period}
                </div>
                <strong style={{ fontSize: 14, fontWeight: 600, ...number }}>
                  {known
                    ? `${integer(value)} ${zh ? "Token" : "tokens"}`
                    : zh
                      ? "不可用"
                      : "Unavailable"}
                </strong>
                {breakdown.map(([id, count]) => {
                  const model = available.models.find(
                    (entry) => entry.id === id,
                  );
                  return (
                    <div key={id} style={{ ...row, gap: 5 }}>
                      <Swatch color={modelColor(id)} />
                      {model && (
                        <HUDLogo
                          source={model.provider}
                          size={12}
                          light={theme.light}
                        />
                      )}
                      <span style={truncate}>{model?.name ?? id}</span>
                      <span style={{ marginLeft: "auto", ...number }}>
                        {integer(count)}
                      </span>
                    </div>
                  );
                })}
              </Tooltip.Content>
            </Tooltip.Portal>
          </Tooltip.Root>
        );
      })}
    </>
  );
}

export const ActivityHeatmap = memo(function ActivityHeatmap({
  data,
  available,
  theme,
}: {
  data: StatisticsHeatmap;
  available: StatisticsResponse["available"];
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const days = zh
    ? ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
    : ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  // The daily heatmap's five greens: an hour's share of the busiest hour,
  // in quarters.
  const color = (value: number) =>
    heatColor(
      value > 0 ? 1 + [0.25, 0.5, 0.75].filter((cut) => value > cut).length : 0,
      theme,
    );
  return (
    <Panel
      area="hours"
      theme={theme}
      title={zh ? "一周里的作息" : "When you work"}
      meta={
        zh
          ? "所选时间里每个星期几、每个小时的 Token"
          : "Tokens by weekday and hour over the range"
      }
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "28px repeat(24,minmax(0,1fr))",
          columnGap: 3,
          rowGap: 4,
          alignItems: "center",
          fontSize: 10,
          color: theme.secondary,
          ["--heatmap-outline" as string]: theme.light
            ? "rgba(29,29,31,.6)"
            : "rgba(245,245,247,.6)",
        }}
      >
        <span />
        {Array.from({ length: 24 }, (_, hour) => (
          <span key={`hour-${hour}`} style={{ height: 12 }}>
            {[0, 6, 12, 18, 23].includes(hour) ? hour : ""}
          </span>
        ))}
        {days.map((day, dayIndex) => (
          <HeatmapRow
            key={day}
            day={day}
            index={dayIndex}
            data={data}
            available={available}
            color={color}
            theme={theme}
          />
        ))}
      </div>
      <div
        style={{
          ...row,
          justifyContent: "flex-end",
          color: theme.secondary,
          fontSize: 10,
          gap: 4,
        }}
      >
        <span>{zh ? "少" : "Less"}</span>
        {[0, 0.2, 0.4, 0.7, 1].map((value) => (
          <span
            key={value}
            aria-hidden
            style={{
              width: 12,
              height: 10,
              borderRadius: 2,
              background: color(value),
            }}
          />
        ))}
        <span>{zh ? "多" : "More"}</span>
      </div>
    </Panel>
  );
});
