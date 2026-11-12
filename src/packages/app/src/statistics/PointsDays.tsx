import { useState } from "react";
import { moneyAmount } from "@agent-hud/ui/format";
import {
  number,
  raisedCard,
  row,
  truncate,
  type StatsTheme,
} from "./StatsTheme";
import { useWidth } from "./dashboard/Panel";
import { addDays } from "./dashboard/model";
import { pointsInSpan } from "./points-days";

const PLOT = 22;
const GAP = 10;

/** Daily points on a detail card. Bar height follows the busiest day in the span; an empty day draws nothing. */
export function PointsDays({
  days,
  first,
  count,
  label,
  theme,
}: {
  days: readonly { date: string; points: number }[];
  first: number;
  count: number;
  label: number;
  theme: StatsTheme;
}) {
  const zh = Boolean(theme.zh);
  const language = zh ? "zh" : "en";
  const { ref, width } = useWidth();
  const [hovered, setHovered] = useState<number | null>(null);
  const shown = pointsInSpan(days, first, count, addDays);
  const peak = Math.max(1, ...shown.map((day) => day.points));
  const plot = Math.max(0, width - label - 52 - GAP * 2);
  const slot = count ? plot / count : 0;
  const bar = Math.max(2, Math.min(24, slot * 0.6));
  const columns = `${label}px ${plot}px 52px`;
  const latest = shown[shown.length - 1];
  const dayLabel = (at: number) =>
    new Date(at).toLocaleDateString(zh ? "zh-CN" : "en-US", {
      month: "short",
      day: "numeric",
    });
  const amount = (points: number) => moneyAmount(points, "credits", language);
  const labelEvery = Math.max(1, Math.ceil(count / Math.max(1, plot / 60)));
  const ticks = [
    ...Array.from({ length: count }, (_, column) => column).filter(
      (column) =>
        column % labelEvery === 0 && column <= count - 1 - labelEvery / 2,
    ),
    count - 1,
  ];
  if (!count || width <= 0) {
    return <div ref={ref} style={{ minWidth: 0, height: 28 }} />;
  }
  return (
    <div
      ref={ref}
      style={{ display: "grid", position: "relative", minWidth: 0 }}
      onMouseLeave={() => setHovered(null)}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: columns,
          columnGap: GAP,
          alignItems: "center",
          height: 28,
        }}
      >
        <span style={{ ...row, minWidth: 0 }}>
          <span style={{ ...truncate, fontSize: 11, color: theme.secondary }}>
            {zh ? "积分" : "Points"}
          </span>
        </span>
        <svg width={plot} height={PLOT} style={{ display: "block" }}>
          {shown.map((day, column) => {
            const height = (day.points / peak) * PLOT;
            const x = column * slot + (slot - bar) / 2;
            return (
              <g key={day.start} onMouseEnter={() => setHovered(column)}>
                <rect
                  x={column * slot}
                  y={0}
                  width={slot}
                  height={PLOT}
                  fill="transparent"
                />
                {height > 0 && (
                  <rect
                    x={x}
                    y={PLOT - Math.max(1.5, height)}
                    width={bar}
                    height={Math.max(1.5, height)}
                    rx={2}
                    fill={theme.text}
                    opacity={0.72}
                  />
                )}
              </g>
            );
          })}
        </svg>
        <span
          style={{
            ...number,
            fontSize: 12,
            fontWeight: 600,
            textAlign: "right",
          }}
        >
          {latest ? amount(latest.points) : "—"}
        </span>
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: columns,
          columnGap: GAP,
          fontSize: 10,
          color: theme.secondary,
        }}
      >
        <span />
        <svg width={plot} height={13} style={{ display: "block" }}>
          {ticks.map((column) => (
            <text
              key={column}
              x={
                column === 0
                  ? 0
                  : column === count - 1
                    ? plot
                    : column * slot + slot / 2
              }
              y={10}
              textAnchor={
                column === 0 ? "start" : column === count - 1 ? "end" : "middle"
              }
              fill={theme.secondary}
            >
              {dayLabel(shown[column].start)}
            </text>
          ))}
        </svg>
        <span style={{ textAlign: "right" }}>{zh ? "当天" : "Day"}</span>
      </div>
      {hovered != null && shown[hovered] && (
        <div
          style={{
            ...raisedCard(theme),
            position: "absolute",
            top: 30,
            left: Math.min(
              label + GAP + hovered * slot,
              Math.max(0, width - 180),
            ),
            fontSize: 11,
          }}
        >
          <strong style={{ fontSize: 12, fontWeight: 600 }}>
            {dayLabel(shown[hovered].start)}
          </strong>
          <span>
            {zh
              ? `${amount(shown[hovered].points)} 积分`
              : `${amount(shown[hovered].points)} points`}
          </span>
        </div>
      )}
    </div>
  );
}
