import type { HUDRect, StripMetrics } from "../geometry";
import type { HUDConfig, HUDEdge } from "../model";

export interface CompactStripMetrics {
  mark: number;
  gap: number;
  pad: number;
  radius: number;
}

export const EDGE_RETRACT_DELAY_MS = 350;

export interface EdgeRetractGate {
  mode: HUDConfig["mode"];
  autoHideAtEdge: boolean;
  preview: boolean;
  open: boolean;
  pinned: boolean;
  pointerInside: boolean;
  cardVisible: boolean;
  dragVisual: boolean;
  hasAlert: boolean;
  hasOpenEnd: boolean;
}

/** Proportional compact strip metrics derived from Simple size & gap settings. */
export function compactStripMetrics(
  config: Pick<HUDConfig, "simpleSize">,
  metrics: Pick<StripMetrics, "gap" | "radius">,
): CompactStripMetrics {
  const size = Math.max(20, Math.min(60, config.simpleSize));
  const mark = Math.round(size * 0.68);
  const gap = Math.min(
    metrics.gap,
    Math.max(4, Math.round(metrics.gap * 0.75)),
  );
  const pad = Math.max(5, Math.round(size * 0.2));
  const radius = Math.min(metrics.radius, Math.round((mark + pad * 2) / 2));
  return { mark, gap, pad, radius };
}

const DEFAULT_COMPACT_METRICS = compactStripMetrics(
  { simpleSize: 26 },
  { gap: 7, radius: 14 },
);

/** Whether a Simple dock is idle and allowed to tuck into a compact edge strip. */
export function canRetractAtEdge(gate: EdgeRetractGate): boolean {
  return (
    gate.mode === "simple" &&
    gate.autoHideAtEdge &&
    !gate.preview &&
    !gate.open &&
    !gate.pinned &&
    !gate.pointerInside &&
    !gate.cardVisible &&
    !gate.dragVisual &&
    !gate.hasAlert &&
    !gate.hasOpenEnd
  );
}

/** Compact Dynamic-style strip flush against `edge`, centered along `strip`. */
export function retractedStripFrame(
  strip: HUDRect,
  edge: HUDEdge,
  vendorCount = 0,
  compact: CompactStripMetrics = DEFAULT_COMPACT_METRICS,
): HUDRect {
  const horiz = edge === "top" || edge === "bottom";
  const longSide = horiz ? strip.width : strip.height;
  const shortSide = horiz ? strip.height : strip.width;
  const thickness = compact.pad * 2 + compact.mark;
  const thin = Math.max(1, Math.min(thickness, shortSide));
  const desiredSpan =
    vendorCount > 0
      ? compact.pad * 2 +
        vendorCount * compact.mark +
        (vendorCount - 1) * compact.gap
      : thickness + 8;
  const span = Math.min(
    longSide,
    Math.max(Math.min(thin, longSide), desiredSpan),
  );
  switch (edge) {
    case "left":
      return {
        x: strip.x,
        y: Math.round(strip.y + (strip.height - span) / 2),
        width: thin,
        height: span,
      };
    case "right":
      return {
        x: strip.x + strip.width - thin,
        y: Math.round(strip.y + (strip.height - span) / 2),
        width: thin,
        height: span,
      };
    case "top":
      return {
        x: Math.round(strip.x + (strip.width - span) / 2),
        y: strip.y,
        width: span,
        height: thin,
      };
    case "bottom":
      return {
        x: Math.round(strip.x + (strip.width - span) / 2),
        y: strip.y + strip.height - thin,
        width: span,
        height: thin,
      };
  }
}
