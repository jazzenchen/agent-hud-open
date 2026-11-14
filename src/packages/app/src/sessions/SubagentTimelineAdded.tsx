import { useFormat, useT } from "@agent-hud/ui/format-context";
import type { Lane } from "./subagent-lanes";
import { compact, cost, numeric, type Theme } from "./SessionTheme";

/** The timeline's right column: the new tokens and the cost each row added. */
export function SubagentTimelineAdded({
  rows,
  rowHeight,
  dim,
  theme,
}: {
  rows: Lane[];
  rowHeight: number;
  /** How strongly a row shows while another is focused. */
  dim: (lane: number) => number;
  theme: Theme;
}) {
  const t = useT();
  const { language } = useFormat();
  return (
    <div style={{ width: 76, flexShrink: 0 }}>
      <div
        style={{
          height: 16,
          fontSize: 11,
          color: theme.secondary,
          textAlign: "right",
        }}
      >
        {t("New · cost")}
      </div>
      {rows.map((lane, i) => (
        <div
          key={lane.key}
          style={{
            ...numeric,
            height: rowHeight,
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            alignItems: "flex-end",
            opacity: dim(i),
          }}
        >
          <span style={{ fontSize: 12, fontWeight: 600 }}>
            {compact(lane.added)}
          </span>
          <span style={{ fontSize: 10.5, color: theme.secondary }}>
            {cost(lane.cost, language, lane.unpriced && lane.cost !== null)}
          </span>
        </div>
      ))}
    </div>
  );
}
