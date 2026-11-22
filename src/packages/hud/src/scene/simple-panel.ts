import type { HUDRect } from "../geometry";
import type { HUDConfig, HUDEdge } from "../model";

/** The room a Simple panel leaves on its parked side, inside its strip (SimpleMeterConfig.edgeMargin). */
export const SIMPLE_EDGE_MARGIN = 3;

/** The Simple panel's own place: its strip without the margin on the parked side (Open IslandRootView.simplePanelRect). */
export function simplePanelRect(strip: HUDRect, edge: HUDEdge): HUDRect {
  const m = SIMPLE_EDGE_MARGIN;
  switch (edge) {
    case "top":
      return { ...strip, y: strip.y + m, height: strip.height - m };
    case "bottom":
      return { ...strip, height: strip.height - m };
    case "left":
      return { ...strip, x: strip.x + m, width: strip.width - m };
    case "right":
      return { ...strip, width: strip.width - m };
  }
}

/** Everything but the Simple panel, as a CSS clip path in the strip's coordinates, the panel in its own corner radius:
 * what grows out of the panel is cut by it, so the two never darken each other (Open IslandRootView.outsidePanel). */
export function outsideSimplePanel(
  strip: HUDRect,
  edge: HUDEdge,
  radius: number,
): string {
  const { x, y, width: w, height: h } = simplePanelRect(strip, edge);
  const r = Math.max(0, Math.min(radius, w / 2, h / 2));
  const arc = (toX: number, toY: number) => `A${r} ${r} 0 0 1 ${toX} ${toY}`;
  const panel =
    `M${x + r} ${y}H${x + w - r}${arc(x + w, y + r)}V${y + h - r}${arc(x + w - r, y + h)}` +
    `H${x + r}${arc(x, y + h - r)}V${y + r}${arc(x + r, y)}Z`;
  return `path(evenodd, "M-100000 -100000H100000V100000H-100000Z${panel}")`;
}

/** The outline a detached card or a Simple panel draws round its body, in
 * points a side. Its frame is border-box, so the body is that much narrower
 * and shorter than the frame: the measuring copy and the frame's height
 * allow for it, or a line that wraps at the narrower width is cut off. */
export function surfaceBorder(
  mode: HUDConfig["mode"],
  detached: boolean,
  simplePeek: boolean,
): number {
  return (detached || mode === "simple") && !simplePeek ? 1 : 0;
}
