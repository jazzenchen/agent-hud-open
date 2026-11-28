import {
  END_SIDES,
  type DockEndsGeometry,
  type EndSide,
} from "../ends/end-geometry.ts";
import type { HUDRect } from "../geometry.ts";
import type { HUDEdge } from "../model.ts";

const clamp = (value: number, low: number, high: number) =>
  Math.min(high, Math.max(low, value));

export interface HoverHitGeometry {
  strip: HUDRect;
  cardVisible: boolean;
  frame: HUDRect;
  heldFrame?: HUDRect | null;
  /** When a Simple dock is folded at rest, only its edge zone takes hover. */
  foldedZone?: HUDRect | null;
  /** Visible dock end pills (`head`, `tail`, `clip`). */
  endRects?: readonly HUDRect[];
}

export function rectContains(
  rect: HUDRect,
  point: { x: number; y: number },
  pad = 0,
): boolean {
  return (
    rect.width > 0 &&
    rect.height > 0 &&
    point.x >= rect.x - pad &&
    point.x <= rect.x + rect.width + pad &&
    point.y >= rect.y - pad &&
    point.y <= rect.y + rect.height + pad
  );
}

/** Collects the rectangles of dock ends currently visible beside the strip. */
export function visibleEndRects(
  geometry: DockEndsGeometry,
  counts: Record<EndSide, number>,
  hidden: boolean,
): HUDRect[] {
  if (hidden) return [];
  return END_SIDES.filter((side) => counts[side] > 0).map(
    (side) => geometry[side],
  );
}

/** ScreenHUD.swift:436-445 (c06 #15): at rest, only the strip (or folded edge
 * zone) and visible dock ends open the panel; the wider `.hud-grab-surface`
 * keeps drag pickup without inflating the hover trigger zone. */
export function isHoverHit(
  point: { x: number; y: number },
  {
    strip,
    cardVisible,
    frame,
    heldFrame,
    foldedZone,
    endRects,
  }: HoverHitGeometry,
): boolean {
  if (endRects?.some((end) => rectContains(end, point, 3))) return true;
  if (cardVisible) {
    return (
      rectContains(frame, point) ||
      rectContains(strip, point) ||
      Boolean(heldFrame && rectContains(heldFrame, point))
    );
  }
  if (foldedZone) return rectContains(foldedZone, point);
  return rectContains(strip, point);
}

/** Anchors GPU scale transitions of an expanded edge panel directly to the
 * resting strip's centre along that edge, even when `frame` is screen-clamped. */
export function expandedOrigin(
  edge: HUDEdge,
  frame: HUDRect,
  strip: HUDRect,
): string {
  if (edge === "top" || edge === "bottom") {
    const x = Math.round(
      clamp(strip.x + strip.width / 2 - frame.x, 0, frame.width),
    );
    return `${x}px ${edge === "top" ? "0px" : "100%"}`;
  }
  const y = Math.round(
    clamp(strip.y + strip.height / 2 - frame.y, 0, frame.height),
  );
  return `${edge === "left" ? "0px" : "100%"} ${y}px`;
}
