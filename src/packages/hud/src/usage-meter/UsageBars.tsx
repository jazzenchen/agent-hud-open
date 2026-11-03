import { HUD_METRICS } from "../model";
import type { RingItem } from "./mark-windows";

/** Horizontal bars for `items`, the first on top, stacked in the middle of a
 * `length` × `height` box; each fills from the left and slides to a new value
 * on the HUD's own curve. */
export function UsageBars({
  items,
  length,
  height,
  thickness,
  gap,
  track,
  reduceMotion = false,
}: {
  items: RingItem[];
  length: number;
  height: number;
  thickness: number;
  gap: number;
  track: string;
  reduceMotion?: boolean;
}) {
  const [x1, y1, x2, y2] = HUD_METRICS.ease;
  return (
    <span
      aria-hidden="true"
      style={{
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap,
        width: length,
        height,
        flexShrink: 0,
      }}
    >
      {items.map((item, i) => (
        <span
          key={i}
          style={{
            display: "block",
            height: thickness,
            borderRadius: thickness / 2,
            background: track,
            overflow: "hidden",
          }}
        >
          <span
            style={{
              display: "block",
              height: "100%",
              width: `${share(item.percent)}%`,
              borderRadius: "inherit",
              background: item.color,
              transition: reduceMotion
                ? undefined
                : `width ${HUD_METRICS.duration}s cubic-bezier(${x1}, ${y1}, ${x2}, ${y2})`,
            }}
          />
        </span>
      ))}
    </span>
  );
}

const share = (percent: number | null) =>
  percent == null || !Number.isFinite(percent)
    ? 0
    : Math.max(0, Math.min(100, percent));
