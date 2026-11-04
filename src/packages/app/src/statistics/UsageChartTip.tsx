import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { StatisticsBucket, StatisticsResponse } from "@agent-hud/protocol";
import type { TokenPlot, TokenPlotColumn } from "../lib/statistics";
import {
  card,
  dateLabel,
  integer,
  modelColor,
  number,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { Swatch } from "./StatsParts";

/** The usage chart's hover card for the inspected bucket: when it starts, its tokens, and each model's. */
export function UsageChartTip({
  data,
  inspected,
  inspectedColumn,
  plot,
  start,
  width,
  theme,
}: {
  data: StatisticsResponse;
  inspected: StatisticsBucket;
  inspectedColumn: TokenPlotColumn;
  plot: TokenPlot;
  start: number;
  width: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <div
      role="tooltip"
      style={{
        ...card(theme),
        position: "absolute",
        zIndex: 10,
        pointerEvents: "none",
        bottom: 108,
        left: Math.max(
          0,
          Math.min(
            width - Math.min(width, 280),
            ((inspectedColumn.start - plot.start) / (plot.end - plot.start)) *
              width -
              130,
          ),
        ),
        width: Math.min(width, 280),
        background: theme.ground,
        boxShadow: "0 5px 18px #0002",
        fontSize: 11,
      }}
    >
      <div style={{ color: theme.secondary, marginBottom: 5 }}>
        {dateLabel(
          Math.max(inspectedColumn.start, start),
          data.query.timeZone,
          zh,
        )}
      </div>
      <strong style={{ ...number, fontSize: 14 }}>
        {integer(inspected.selectedTokens)} {zh ? "Token" : "tokens"}
      </strong>
      {data.models
        .filter((model) => (inspected.models[model.id] ?? 0) > 0)
        .map((model) => (
          <div key={model.id} style={{ ...row, marginTop: 5 }}>
            <Swatch color={modelColor(model.id)} />
            <HUDLogo source={model.provider} size={11} light={theme.light} />
            <span style={truncate}>{model.name}</span>
            <span style={{ marginLeft: "auto", ...number }}>
              {integer(inspected.models[model.id])}
            </span>
          </div>
        ))}
      {inspectedColumn.partial && (
        <div style={{ color: theme.secondary, marginTop: 5 }}>
          {zh
            ? "仅统计所选范围内的部分时段"
            : "Partial period within the selected range"}
        </div>
      )}
    </div>
  );
}
