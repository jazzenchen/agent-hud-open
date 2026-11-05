// Where the dock's two ends sit. The head holds the requests that wait for
// the person, the tail the messages that came in. Pure geometry, no DOM.
import type { HUDRect, StripMetrics } from "../geometry.ts";
import type { HUDConfig, HUDEdge } from "../model.ts";
import { simplePanelRect } from "../scene/simple-panel.ts";

export type EndSide = "head" | "tail";
export const END_SIDES: readonly EndSide[] = ["head", "tail"];

export interface DockEndsGeometry {
  head: HUDRect;
  tail: HUDRect;
  /** How far the notch grows on each side to hold an end that shows. */
  wings: Record<EndSide, number>;
  /** The pill's short side. */
  thick: number;
}

/** Between the dock and an end, and between the two ends. */
const GAP = 6;
/** Room around an end inside the notch's wing. */
const WING_PADDING = 7;

const clamp = (value: number, low: number, high: number) =>
  Math.min(high, Math.max(low, value));

/** An end of `thick` holds its icon alone, a little smaller than the dock's
 * logos; the number sits in a small badge on its top-right edge. */
export function endBadge(thick: number, count: number) {
  const size = Math.max(12, Math.round(thick * 0.46));
  return {
    icon: Math.max(8, Math.round(thick * 0.4)),
    size,
    font: Math.max(8, Math.round(size * 0.68)),
    label: count > 99 ? "99+" : String(count),
    /** The badge's centre, from the end's top-left corner. */
    x: thick * 0.85,
    y: thick * 0.15,
  };
}

/** The ends of the dock (`strip`) on `edge`, each a circle: the head before its first mark,
 * the tail after its last. Where the screen has no room on one side, both go
 * to the other, the head always first. In notch mode they sit inside the
 * notch, which grows sideways (`wings`) for the ones that show. */
export function dockEnds(
  strip: HUDRect,
  edge: HUDEdge,
  config: Pick<HUDConfig, "mode" | "simpleSize">,
  metrics: Pick<StripMetrics, "mark" | "padding">,
  screen: { width: number; height: number },
  counts: Record<EndSide, number>,
): DockEndsGeometry {
  const notch = config.mode === "notch";
  const across = notch || edge === "top" || edge === "bottom";
  const band =
    config.mode === "simple" ? simplePanelRect(strip, edge) : { ...strip };
  // A Simple dock's circle is as thick as its row of rings with their
  // padding, which a side edge's wider panel (numbers beside the rings)
  // leaves the same.
  const thick = notch
    ? clamp(strip.height - 12, 18, 26)
    : metrics.mark +
      2 * Math.max(3, Math.round(clamp(config.simpleSize, 20, 60) * 0.18));
  // Each end is a circle as thick as the dock.
  const length = { head: thick, tail: thick };
  // The circle's place across the dock: centred on the dock's own band, or
  // on a side edge's Simple panel, on its column of rings.
  const cross = across
    ? band.y + band.height / 2 - thick / 2
    : band.x + metrics.padding.left + metrics.mark / 2 - thick / 2;
  const start = across ? strip.x : strip.y;
  const end = across ? strip.x + strip.width : strip.y + strip.height;
  const limit = across ? screen.width : screen.height;
  const wings = {
    head: counts.head > 0 ? length.head + WING_PADDING * 2 : 0,
    tail: counts.tail > 0 ? length.tail + WING_PADDING * 2 : 0,
  };
  let along: Record<EndSide, number>;
  if (notch) {
    along = {
      head: start - (length.head + WING_PADDING * 2) / 2 - length.head / 2,
      tail: end + (length.tail + WING_PADDING * 2) / 2 - length.tail / 2,
    };
  } else if (start - GAP - length.head < 0) {
    along = { head: end + GAP, tail: end + GAP + length.head + GAP };
  } else if (end + GAP + length.tail > limit) {
    along = {
      tail: start - GAP - length.tail,
      head: start - GAP - length.tail - GAP - length.head,
    };
  } else {
    along = { head: start - GAP - length.head, tail: end + GAP };
  }
  const rect = (side: EndSide): HUDRect =>
    across
      ? { x: along[side], y: cross, width: length[side], height: thick }
      : { x: cross, y: along[side], width: thick, height: length[side] };
  return {
    head: rect("head"),
    tail: rect("tail"),
    wings: notch ? wings : { head: 0, tail: 0 },
    thick,
  };
}

/** The position along the dock where a card opened from `rect` centres. */
export function endAnchor(rect: HUDRect, edge: HUDEdge): number {
  return edge === "top" || edge === "bottom"
    ? rect.x + rect.width / 2
    : rect.y + rect.height / 2;
}

/** A rounded rect's outline as one path, from the middle of its top side,
 * clockwise, with its length: the fade ring is drawn and emptied along it. */
export function ringPath(
  width: number,
  height: number,
  inset: number,
): { d: string; length: number } {
  const x = inset,
    y = inset,
    w = Math.max(0, width - inset * 2),
    h = Math.max(0, height - inset * 2);
  const r = Math.min(w, h) / 2;
  const arc = (toX: number, toY: number) => `A${r} ${r} 0 0 1 ${toX} ${toY}`;
  const d =
    `M${x + w / 2} ${y}H${x + w - r}${arc(x + w, y + r)}V${y + h - r}` +
    `${arc(x + w - r, y + h)}H${x + r}${arc(x, y + h - r)}V${y + r}` +
    `${arc(x + r, y)}H${x + w / 2}`;
  return { d, length: 2 * (w - 2 * r) + 2 * (h - 2 * r) + 2 * Math.PI * r };
}
