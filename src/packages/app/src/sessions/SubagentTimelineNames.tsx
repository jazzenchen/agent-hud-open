import type { Lane } from "./subagent-lanes";
import { clipped, plain, row, type Theme } from "./SessionTheme";

/** The timeline's left column: each row's name, a button that focuses that row. */
export function SubagentTimelineNames({
  rows,
  rowHeight,
  focus,
  onFocus,
  dim,
  theme,
}: {
  rows: Lane[];
  rowHeight: number;
  focus: number | null;
  onFocus: (lane: number | null) => void;
  /** How strongly a row shows while another is focused. */
  dim: (lane: number) => number;
  theme: Theme;
}) {
  return (
    // Narrower in a narrow column, so the plot keeps its room.
    <div style={{ width: "clamp(120px, 34%, 200px)", flexShrink: 0 }}>
      <div style={{ height: 16, fontSize: 11, color: theme.secondary }}>
        agent
      </div>
      {rows.map((lane, i) => (
        <button
          key={lane.key}
          type="button"
          onClick={() => onFocus(focus === i ? null : i)}
          aria-pressed={focus === i}
          aria-label={`${lane.name}${lane.sub ? `, ${lane.sub}` : ""}`}
          style={{
            ...plain,
            ...row,
            gap: 7,
            width: "100%",
            height: rowHeight,
            paddingLeft: Math.min(lane.depth, 3) * 10,
            boxSizing: "border-box",
            borderRadius: 6,
            background: focus === i ? theme.pick : "transparent",
            opacity: dim(i),
            transition: "opacity 150ms ease",
          }}
        >
          <span
            aria-hidden="true"
            style={{
              width: 3,
              height: 22,
              borderRadius: 2,
              background: lane.color,
              flexShrink: 0,
            }}
          />
          <span
            style={{
              display: "flex",
              flexDirection: "column",
              minWidth: 0,
            }}
          >
            <span
              style={{
                ...clipped,
                fontSize: 12,
                fontWeight: lane.depth === 0 ? 600 : 500,
              }}
            >
              {lane.name}
            </span>
            {lane.sub && (
              <span
                style={{
                  ...clipped,
                  fontSize: 10.5,
                  color: theme.secondary,
                }}
              >
                {lane.sub}
              </span>
            )}
          </span>
        </button>
      ))}
    </div>
  );
}
