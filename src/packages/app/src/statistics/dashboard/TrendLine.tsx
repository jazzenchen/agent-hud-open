import type { StatsTheme } from "../StatsTheme";
import { smoothPath } from "./curves";
import { useWidth } from "./Panel";

/** A figure's trend across the range: a quiet line with a wash under it and
 * a dot on the latest value. A null leaves its column out. */
export function TrendLine({
  values,
  theme,
  height = 28,
}: {
  values: (number | null)[];
  theme: StatsTheme;
  height?: number;
}) {
  const { ref, width } = useWidth();
  const known = values.filter((value): value is number => value !== null);
  const low = Math.min(0, ...known);
  const high = Math.max(...known, 0);
  const span = high - low || 1;
  const step = values.length > 1 ? (width - 6) / (values.length - 1) : 0;
  const points = values.flatMap((value, index) =>
    value === null
      ? []
      : [
          [3 + index * step, 3 + (1 - (value - low) / span) * (height - 6)] as [
            number,
            number,
          ],
        ],
  );
  const last = points[points.length - 1];
  const line = theme.light ? "rgba(0,0,0,.42)" : "rgba(255,255,255,.5)";
  return (
    <div ref={ref} style={{ height, minWidth: 0 }} aria-hidden>
      {width > 0 && points.length > 1 && (
        <svg width={width} height={height} style={{ display: "block" }}>
          <path
            d={`${smoothPath(points)} L${last[0]},${height} L${points[0][0]},${height} Z`}
            fill={line}
            opacity={0.12}
          />
          <path
            d={smoothPath(points)}
            fill="none"
            stroke={line}
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <circle cx={last[0]} cy={last[1]} r={2.5} fill={line} />
        </svg>
      )}
    </div>
  );
}
