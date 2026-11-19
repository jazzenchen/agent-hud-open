// Where the folded dock's bar lies. Pure geometry, no DOM: the tests run it
// in Node, so it leans on nothing but its arguments.
import type { HUDRect, StripMetrics } from "../geometry.ts";
import type { HUDEdge } from "../model.ts";

/** How far the bar floats off the screen edge, the way a phone's own bar
 * sits clear of the glass instead of on it. */
export function drawerHandleInset(metrics: StripMetrics): number {
  return Math.max(5, Math.round(metrics.mark * 0.28));
}

/** One mark thick, a few marks long, clear of the screen edge the dock hangs
 * from, and centred on where the dock's marks were. */
export function drawerHandleRect(
  edge: HUDEdge,
  strip: HUDRect,
  metrics: StripMetrics,
): HUDRect {
  // `horizontal` in geometry.ts, whose module carries the whole model.
  const across = edge === "top" || edge === "bottom";
  const thickness = Math.max(4, Math.round(metrics.mark * 0.26));
  const inset = drawerHandleInset(metrics);
  const run = across ? strip.width : strip.height;
  const length = Math.min(run, Math.max(44, Math.round(metrics.mark * 3.4)));
  const centreX = strip.x + (strip.width - length) / 2;
  const centreY = strip.y + (strip.height - length) / 2;
  if (edge === "top")
    return {
      x: centreX,
      y: strip.y + inset,
      width: length,
      height: thickness,
    };
  if (edge === "bottom")
    return {
      x: centreX,
      y: strip.y + strip.height - thickness - inset,
      width: length,
      height: thickness,
    };
  if (edge === "left")
    return {
      x: strip.x + inset,
      y: centreY,
      width: thickness,
      height: length,
    };
  return {
    x: strip.x + strip.width - thickness - inset,
    y: centreY,
    width: thickness,
    height: length,
  };
}
