import { quotaPercentLabel, type QuotaDisplay } from "@agent-hud/ui/format";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import { card, dateLabel, number, row, type StatsTheme } from "./StatsTheme";

/** The card over a quota chart where the pointer is: the time, then each
 * window's reading there in its colour. */
export function QuotaChartTip({
  shown,
  left,
  bottom,
  display,
  theme,
  timeZone,
}: {
  shown: {
    reading: [number, number];
    window: QuotaHistoryWindow;
    color: string;
  }[];
  left: number;
  bottom: number;
  display: QuotaDisplay;
  theme: StatsTheme;
  timeZone: string;
}) {
  return (
    <div
      role="tooltip"
      style={{
        ...card(theme),
        position: "absolute",
        zIndex: 10,
        pointerEvents: "none",
        bottom,
        left,
        width: 200,
        padding: "8px 10px",
        background: theme.ground,
        boxShadow: "0 5px 18px #0002",
        fontSize: 11,
        display: "grid",
        gap: 3,
      }}
    >
      <div style={{ color: theme.secondary }}>
        {dateLabel(shown[0].reading[0], timeZone, Boolean(theme.zh))}
      </div>
      {shown.map(({ reading, window, color }) => (
        <div key={window.id} style={{ ...row, gap: 6 }}>
          <span
            aria-hidden
            style={{
              width: 7,
              height: 7,
              borderRadius: 2,
              background: color,
              flexShrink: 0,
            }}
          />
          <span style={{ color: theme.secondary }}>
            {window.shortLabel ?? window.name}
          </span>
          <strong style={{ ...number, marginLeft: "auto" }}>
            {quotaPercentLabel(reading[1], display)}
          </strong>
        </div>
      ))}
    </div>
  );
}
