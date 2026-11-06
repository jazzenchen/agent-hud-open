import { largestPositions, type StackedBar } from "../lib/session-charts";
import { Halo, stackedRects, useWidth } from "./SessionCharts";
import { compact, numeric, type Theme } from "./SessionTheme";

/**
 * A row's recent turns (the phone's `TurnSparkline`): each turn's new tokens
 * stacked by kind, the sub-agents' part faded, a compaction as an amber line
 * before its turn, and the largest turn labelled with its value so the bars
 * have a scale. No turn is marked as running: lists show no session state
 * (owner, 2026-10-26).
 */
export function TurnSparkline({
  bars,
  theme,
  height = 34,
  label,
}: {
  bars: StackedBar[];
  theme: Theme;
  height?: number;
  label: string;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const headroom = 12;
  const slot = width / Math.max(bars.length, 12);
  const barWidth = Math.max(1.5, slot - 2);
  const peak = Math.max(1, ...bars.map((bar) => bar.total));
  const scale = (height - headroom - 1) / peak;
  const [largest] = largestPositions(
    bars.map((bar) => bar.total),
    1,
  );
  const tops: number[] = [];
  return (
    <div
      ref={ref}
      style={{ height, position: "relative", minWidth: 0 }}
      role="img"
      aria-label={label}
    >
      {width > 0 && (
        <svg
          width={width}
          height={height}
          style={{ display: "block", overflow: "visible" }}
        >
          {bars.map((bar, index) => {
            const x = index * slot + (slot - barWidth) / 2;
            const { rects, top } = stackedRects(
              bar,
              x,
              barWidth,
              height - 1,
              scale,
              1.5,
              theme,
              `b${index}`,
              false,
            );
            tops[index] = top;
            return (
              <g key={index}>
                {bar.compacted && (
                  <line
                    x1={index * slot}
                    x2={index * slot}
                    y1={headroom - 2}
                    y2={height - 1}
                    stroke={theme.compaction}
                    strokeWidth={1}
                    strokeDasharray="2 2"
                  />
                )}
                {rects}
              </g>
            );
          })}
          <rect
            x={0}
            y={height - 0.5}
            width={width}
            height={0.5}
            fill={theme.divider}
          />
          {largest !== undefined && (
            <Halo
              theme={theme}
              x={Math.min(Math.max(largest * slot + slot / 2, 12), width - 12)}
              y={Math.max(9, (tops[largest] ?? height) - 3)}
              textAnchor="middle"
              fill={theme.secondary}
              style={numeric}
            >
              {compact(bars[largest].total)}
            </Halo>
          )}
        </svg>
      )}
    </div>
  );
}
