// A Simple dock at rest. While nothing needs it, it can step back against
// its edge: down to its logos, into a bar, or out of sight. A zone flush
// against the edge stays live for the pointer, so reaching the edge there
// brings the dock back, as does anything that wants the person: a request,
// a message, a card. Pure, so the tests run it in Node.
import type { HUDRect, StripMetrics } from "../geometry.ts";
import type { HUDConfig, HUDEdge, HUDVendor, RestStyle } from "../model.ts";
import { statusColor } from "../mark-colors.ts";
import { hudGround } from "./hud-ground.ts";

/** How deep the edge stays live where the dock hides, in points. */
export const REST_EDGE = 4;
/** How long the dock takes to fold or unfold, in milliseconds. */
export const REST_MS = 240;

/** The ways a dock rests that fold it. */
export type FoldStyle = Exclude<RestStyle, "dock">;

/** How this screen's dock rests, or null where it never folds: a notch, a
 * dock with Auto-hide off, or one that stays whole (anything else saved
 * counts as that). */
export function foldStyle(
  config: Pick<HUDConfig, "mode" | "autoHide" | "restStyle">,
): FoldStyle | null {
  if (config.mode !== "simple" || !config.autoHide) return null;
  const style = config.restStyle;
  return style === "logos" || style === "bar" || style === "hidden"
    ? style
    : null;
}

/** What keeps the dock out. */
export interface RestGate {
  /** The panel is open, opening, closing or pinned. */
  open: boolean;
  pointerInside: boolean;
  /** A card is out beside the dock. */
  cardVisible: boolean;
  /** A request or message is on show, or a request or session waits for
   * an answer. */
  alert: boolean;
  /** One of the dock's ends holds something, shows its panel, or has a
   * card flying to it. */
  ends: boolean;
  dragging: boolean;
  /** The guide's window stands beside the dock. */
  guide: boolean;
  /** A text field in the HUD has the keyboard. */
  typing: boolean;
}

/** Whether the dock may fold now: nothing is out, nothing waits, and the
 * pointer is away. */
export function mayRest(gate: RestGate): boolean {
  return !(
    gate.open ||
    gate.pointerInside ||
    gate.cardVisible ||
    gate.alert ||
    gate.ends ||
    gate.dragging ||
    gate.guide ||
    gate.typing
  );
}

/** A dock resting as its logos: each logo alone, without its ring or
 * figures, at the size it has in the dock, the strip drawn close round them. */
export function logoStrip(
  metrics: Pick<StripMetrics, "logo" | "gap" | "radius">,
) {
  const mark = metrics.logo;
  const pad = Math.max(3, Math.round(mark * 0.3));
  return {
    mark,
    pad,
    gap: Math.max(3, Math.round(metrics.gap * 0.75)),
    radius: Math.min(metrics.radius, Math.round(mark / 2 + pad)),
  };
}

const across = (edge: HUDEdge) => edge === "top" || edge === "bottom";

/** A rect `length` long and `depth` deep, flush against `edge` where the
 * strip meets it, centred on the strip and kept on the screen. */
export function againstEdge(
  strip: HUDRect,
  edge: HUDEdge,
  screen: { width: number; height: number },
  length: number,
  depth: number,
): HUDRect {
  const run = across(edge) ? screen.width : screen.height;
  const long = Math.max(1, Math.min(Math.round(length), run));
  const middle = across(edge)
    ? strip.x + strip.width / 2
    : strip.y + strip.height / 2;
  const start = Math.max(
    0,
    Math.min(run - long, Math.round(middle - long / 2)),
  );
  const deep = Math.max(1, Math.round(depth));
  switch (edge) {
    case "top":
      return { x: start, y: strip.y, width: long, height: deep };
    case "bottom":
      return {
        x: start,
        y: strip.y + strip.height - deep,
        width: long,
        height: deep,
      };
    case "left":
      return { x: strip.x, y: start, width: deep, height: long };
    case "right":
      return {
        x: strip.x + strip.width - deep,
        y: start,
        width: deep,
        height: long,
      };
  }
}

const clamp = (value: number, low: number, high: number) =>
  Math.max(low, Math.min(high, Number.isFinite(value) ? value : low));

/** What the folded dock shows, flush against its edge: the logos' strip,
 * the bar, or, hidden, a strip of the edge as long as the dock. */
export function restShape(
  style: FoldStyle,
  config: Pick<HUDConfig, "restBarLength" | "restBarThickness">,
  strip: HUDRect,
  edge: HUDEdge,
  screen: { width: number; height: number },
  metrics: Pick<StripMetrics, "logo" | "gap" | "radius">,
  count: number,
): HUDRect {
  if (style === "logos") {
    const logos = logoStrip(metrics);
    const deep = logos.mark + logos.pad * 2;
    const long =
      count > 0
        ? logos.pad * 2 + count * logos.mark + (count - 1) * logos.gap
        : deep;
    return againstEdge(strip, edge, screen, long, deep);
  }
  if (style === "bar")
    return againstEdge(
      strip,
      edge,
      screen,
      clamp(config.restBarLength, 16, 400),
      clamp(config.restBarThickness, 1, 12),
    );
  const long = across(edge) ? strip.width : strip.height;
  return againstEdge(strip, edge, screen, long, REST_EDGE);
}

/** Where the pointer finds the folded dock: what it shows, made at least
 * `REST_EDGE` deep, still flush against the edge. */
export function restZone(shape: HUDRect, edge: HUDEdge): HUDRect {
  const deep = Math.max(REST_EDGE, across(edge) ? shape.height : shape.width);
  switch (edge) {
    case "top":
      return { ...shape, height: deep };
    case "bottom":
      return { ...shape, y: shape.y + shape.height - deep, height: deep };
    case "left":
      return { ...shape, width: deep };
    case "right":
      return { ...shape, x: shape.x + shape.width - deep, width: deep };
  }
}

/** The bar's paint: the level of the tightest window when one runs low;
 * else the colour chosen; else (automatic) the HUD's text colour for its
 * appearance, as see-through as its transparency lets the dock be but never
 * gone. Its edge is the dock's ground, so it reads on any wallpaper. */
export function barPaint(
  look: Pick<HUDConfig, "restBarColor" | "solidTransparency">,
  levels: readonly HUDVendor["level"][],
  light: boolean,
): { fill: string; edge: string } {
  const alpha = Math.max(0.3, 1 - look.solidTransparency);
  const edge = hudGround(light, alpha).fill;
  const worst = levels.includes("critical")
    ? "critical"
    : levels.includes("warning")
      ? "warning"
      : null;
  if (worst) return { fill: statusColor(worst), edge };
  if (/^#[0-9a-f]{3,8}$/i.test(look.restBarColor))
    return { fill: look.restBarColor, edge };
  const ink = light ? "28,28,30" : "242,242,247";
  return { fill: `rgba(${ink},${(0.8 * alpha).toFixed(2)})`, edge };
}

/** Everything the scene draws of a dock that folds. */
export interface Rest {
  style: FoldStyle;
  edge: HUDEdge;
  /** Folded now. */
  folded: boolean;
  /** What it shows folded, and where the pointer finds it. */
  shape: HUDRect;
  zone: HUDRect;
  logos: ReturnType<typeof logoStrip>;
  /** The bar's paint, for a dock that rests as a bar. */
  bar: ReturnType<typeof barPaint> | null;
  reduceMotion: boolean;
}
