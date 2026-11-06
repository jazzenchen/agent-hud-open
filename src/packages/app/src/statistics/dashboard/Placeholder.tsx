import type { CSSProperties } from "react";
import { Skeleton } from "@agent-hud/ui/components/skeleton";
import type { StatsTheme } from "../StatsTheme";

/** A grey bar that pulses where content is still coming: shadcn's Skeleton
 * in the page's own grey. */
export function Bar({
  width = "100%",
  height = 12,
  radius = 4,
  theme,
  style,
}: {
  width?: number | string;
  height?: number | string;
  radius?: number;
  theme: StatsTheme;
  style?: CSSProperties;
}) {
  return (
    <Skeleton
      aria-hidden
      style={{
        width,
        height,
        borderRadius: radius,
        background: theme.track,
        ...style,
      }}
    />
  );
}

/** Columns of uneven height, the shape of a chart still loading. */
export function ChartPlaceholder({
  height,
  columns = 28,
  theme,
}: {
  height: number;
  columns?: number;
  theme: StatsTheme;
}) {
  return (
    <div
      aria-hidden
      className="animate-pulse"
      style={{
        height,
        display: "flex",
        alignItems: "flex-end",
        gap: 4,
      }}
    >
      {Array.from({ length: columns }, (_, index) => (
        <span
          key={index}
          style={{
            flex: 1,
            height: `${28 + ((index * 37) % 61)}%`,
            borderRadius: 3,
            background: theme.track,
          }}
        />
      ))}
    </div>
  );
}

/** A heatmap still loading: a dot in each cell, a wave rippling across
 * them corner to corner. */
export function GridPlaceholder({
  columns,
  rows = 7,
  cell,
  gap = 3,
  theme,
}: {
  columns: number;
  rows?: number;
  /** A cell's height, points; its width follows the columns. */
  cell: number;
  gap?: number;
  theme: StatsTheme;
}) {
  return (
    <div
      aria-hidden
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${columns},minmax(0,1fr))`,
        gap,
      }}
    >
      {Array.from({ length: columns * rows }, (_, index) => {
        const column = index % columns;
        const row = Math.floor(index / columns);
        return (
          <span
            key={index}
            style={{ height: cell, display: "grid", placeItems: "center" }}
          >
            <span
              className="stats-dot"
              style={{
                width: Math.max(3, cell * 0.55),
                height: Math.max(3, cell * 0.55),
                borderRadius: "50%",
                background: theme.secondary,
                animationDelay: `${((column + row) % 28) * 55}ms`,
              }}
            />
          </span>
        );
      })}
    </div>
  );
}

/** The day heatmap still loading: its weeks in grey, and its four facts. */
export function CalendarPlaceholder({
  columns,
  width,
  label,
  gap,
  theme,
}: {
  columns: number;
  width: number;
  label: number;
  gap: number;
  theme: StatsTheme;
}) {
  const cell = Math.max(
    8,
    Math.min(24, Math.floor((width - label) / columns) - gap),
  );
  return (
    <div style={{ display: "grid", gap: 14 }}>
      <div style={{ paddingLeft: label, paddingTop: 16 }}>
        <GridPlaceholder
          columns={columns}
          cell={cell}
          gap={gap}
          theme={theme}
        />
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(4,minmax(0,1fr))",
          gap: 12,
          paddingTop: 12,
          borderTop: `1px solid ${theme.divider}`,
        }}
      >
        {[0, 1, 2, 3].map((index) => (
          <div key={index} style={{ display: "grid", gap: 6 }}>
            <Bar width={72} height={10} theme={theme} />
            <Bar width={56} height={16} theme={theme} />
          </div>
        ))}
      </div>
    </div>
  );
}

/** The Usage page before anything is read: its filters, figures, heatmap
 * and chart in grey, in the page's grid. */
export function PagePlaceholder({ theme }: { theme: StatsTheme }) {
  const frame = {
    borderRadius: 10,
    border: `1px solid ${theme.border}`,
    background: theme.card,
    padding: 16,
    display: "grid",
    gap: 12,
  } as const;
  return (
    <div
      className="statistics-native stats-dashboard"
      style={{ display: "grid", gap: 14, background: theme.ground }}
    >
      <div style={{ display: "flex", gap: 8, padding: "6px 0 12px" }}>
        <Bar width={84} height={28} radius={14} theme={theme} />
        <Bar width={110} height={28} radius={14} theme={theme} />
        <Bar width={90} height={28} radius={14} theme={theme} />
        <span style={{ flex: 1 }} />
        <Bar width={260} height={28} radius={7} theme={theme} />
      </div>
      <div className="stats-figures">
        {[0, 1, 2, 3, 4].map((index) => (
          <div key={index} style={{ ...frame, gap: 6 }}>
            <Bar width={80} height={10} theme={theme} />
            <Bar width={96} height={24} theme={theme} />
            <Bar height={22} theme={theme} />
          </div>
        ))}
      </div>
      <div style={frame}>
        <Bar width={140} height={12} theme={theme} />
        <GridPlaceholder columns={53} cell={14} theme={theme} />
      </div>
      <div style={frame}>
        <Bar width={100} height={12} theme={theme} />
        <ChartPlaceholder height={180} columns={30} theme={theme} />
      </div>
    </div>
  );
}
