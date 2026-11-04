import { memo } from "react";
import { Card } from "@agent-hud/ui/components/card";
import type { StatisticsPeriod } from "@agent-hud/protocol";
import {
  card,
  compact,
  costHelp,
  costText,
  number,
  row,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { Coverage, Heading } from "./StatsParts";
import { ModelList } from "./ModelList";

const periodTitles = {
  today: ["Today", "今日"],
  "7d": ["Last 7 days", "近 7 天"],
  "30d": ["Last 30 days", "近 30 天"],
} as const;

function RecentPeriodColumn({
  period,
  index,
  theme,
}: {
  period: StatisticsPeriod;
  index: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const animatedTokens = useAnimatedNumber(period.totals.selectedTokens);
  const promptTotal =
    period.totals.tokens.cacheRead +
    period.totals.tokens.cacheWrite +
    period.totals.tokens.input;
  const cacheHitRate =
    promptTotal > 0
      ? `${Math.round((period.totals.tokens.cacheRead / promptTotal) * 100)}%`
      : "—";
  return (
    <div
      style={{
        padding:
          index === 0 ? "0 14px 0 0" : index === 2 ? "0 0 0 14px" : "0 14px",
        borderLeft: index ? `1px solid ${theme.divider}` : 0,
        minWidth: 0,
      }}
    >
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          fontSize: 12,
          fontWeight: 600,
          marginBottom: 6,
        }}
      >
        <span>{periodTitles[period.id][zh ? 1 : 0]}</span>
        <span style={{ fontSize: 15, ...number }}>
          {period.coverage.state === "unavailable"
            ? "—"
            : compact(animatedTokens)}
        </span>
      </div>
      <div
        title={costHelp(period.totals.cost, zh)}
        style={{ color: theme.secondary, fontSize: 11, marginBottom: 4 }}
      >
        {zh
          ? `按 API 价格估算 ${costText(period.totals.cost, zh)}`
          : `At API prices ${costText(period.totals.cost, zh)}`}
      </div>
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          color: theme.secondary,
          fontSize: 11,
          marginBottom: 6,
        }}
      >
        <span>{zh ? "缓存命中" : "Cache hit"}</span>
        <span style={number}>{cacheHitRate}</span>
      </div>
      <ModelList
        models={period.models}
        total={period.totals.selectedTokens}
        theme={theme}
      />
      <Coverage coverage={period.coverage} theme={theme} />
    </div>
  );
}

export const RecentUsage = memo(function RecentUsage({
  periods,
  label,
  theme,
}: {
  periods: StatisticsPeriod[];
  label: string;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  return (
    <Card className="statistics-recent-card" style={card(theme)}>
      <Heading theme={theme} trailing={label}>
        {zh ? "近期用量" : "Recent usage"}
      </Heading>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3,minmax(0,1fr))",
        }}
      >
        {periods.map((period, index) => (
          <RecentPeriodColumn
            key={period.id}
            period={period}
            index={index}
            theme={theme}
          />
        ))}
      </div>
    </Card>
  );
});
