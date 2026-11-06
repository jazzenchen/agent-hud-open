import { cycleLevel } from "@agent-hud/hud/panel/quota-groups";
import type { StatsTheme } from "../StatsTheme";
import type { QuotaDay } from "./quota-days";

const PLOT = 22;

/** A share's colour: quiet while it is fine, amber from 70%, red from 90%. */
export function levelFill(used: number, theme: StatsTheme): string {
  const level = cycleLevel(used);
  if (level === "critical") return theme.light ? "#e0352b" : "#ff453a";
  if (level === "warning") return theme.light ? "#c27c00" : "#ff9f0a";
  return theme.secondary;
}

/** One window's days as bars in their slots, over a faint 0–100% track. A
 * weekly window stands as full as it was at the day's end, the day's own
 * share darker on top, with a tick where it reset; a short one shows its
 * peak. */
export function QuotaDayBars({
  days,
  long,
  plot,
  slot,
  bar,
  theme,
  onHover,
}: {
  days: QuotaDay[];
  long: boolean;
  plot: number;
  slot: number;
  bar: number;
  theme: StatsTheme;
  onHover: (column: number) => void;
}) {
  return (
    <svg width={plot} height={PLOT} style={{ display: "block" }}>
      {days.map((value, column) => {
        const x = column * slot + (slot - bar) / 2;
        const total = long ? (value.level ?? 0) : (value.value ?? 0);
        const height = (Math.min(100, total) / 100) * PLOT;
        const own = long
          ? (Math.min(value.value ?? 0, total) / 100) * PLOT
          : height;
        const fill = levelFill(value.level ?? 0, theme);
        return (
          <g key={value.start} onMouseEnter={() => onHover(column)}>
            <rect
              x={column * slot}
              y={0}
              width={slot}
              height={PLOT}
              fill="transparent"
            />
            <rect
              x={x}
              y={0}
              width={bar}
              height={PLOT}
              rx={2}
              fill={theme.track}
              opacity={0.45}
            />
            {height > 0 && (
              <rect
                x={x}
                y={PLOT - Math.max(1.5, height)}
                width={bar}
                height={Math.max(1.5, height)}
                rx={2}
                fill={fill}
                opacity={long ? 0.35 : 1}
              />
            )}
            {long && own > 0 && (
              <rect
                x={x}
                y={PLOT - height}
                width={bar}
                height={Math.max(1.5, own)}
                rx={2}
                fill={fill}
              />
            )}
            {long &&
              value.resets.map((reset) => {
                const at =
                  column * slot +
                  ((reset - value.start) / (value.end - value.start)) * slot;
                return (
                  <line
                    key={reset}
                    x1={at}
                    x2={at}
                    y1={-2}
                    y2={PLOT + 2}
                    stroke={theme.secondary}
                    strokeWidth={1}
                    opacity={0.7}
                  />
                );
              })}
          </g>
        );
      })}
    </svg>
  );
}
