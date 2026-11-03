/** How usage is drawn: concentric rings (年轮), one ring split into a segment
 * per item (分段环), horizontal bars (进度条), or the number alone (数字). */
export type UsageStyle = "rings" | "segments" | "bars" | "numbers";
export type RingStyle = Extract<UsageStyle, "rings" | "segments">;

export const USAGE_STYLES: readonly UsageStyle[] = [
  "rings",
  "segments",
  "bars",
  "numbers",
];

/** A ring or segmented ring draws at most this many items; the rest stay in the panel's list. */
export const MAX_USAGE_ITEMS = 4;

export function usageStyle(value: unknown): UsageStyle | null {
  return USAGE_STYLES.find((style) => style === value) ?? null;
}

export function isRingStyle(style: UsageStyle): style is RingStyle {
  return style === "rings" || style === "segments";
}

/** Where one item sits: a circle of `radius` drawn `width` thick from `start`
 * to `end`, in degrees clockwise from 12 o'clock. Items fill clockwise from
 * `start`. */
export interface UsageArc {
  radius: number;
  width: number;
  start: number;
  end: number;
}

/** A ring's box: its outer diameter, the thickest line it draws, the
 * diameter its rings leave free in the middle, and the clear space between
 * two segments. */
export interface RingSpec {
  size: number;
  width: number;
  hole: number;
  gap: number;
}

/** Segment spans for 2, 3 and 4 items: halves top and bottom, thirds with one
 * on top, quarters; the first item on top (top left for four), the rest
 * clockwise. */
const SPANS: Record<number, [number, number][]> = {
  2: [
    [-90, 90],
    [90, 270],
  ],
  3: [
    [-60, 60],
    [60, 180],
    [180, 300],
  ],
  4: [
    [-90, 0],
    [0, 90],
    [90, 180],
    [180, 270],
  ],
};

/** The arcs `count` items take in `style`. Rings run from the outside in, the
 * first outermost, and thin out to fit the band outside `hole`, never wider
 * than `width`, with half a line between rings. Segments share one ring of
 * `width`, with `gap` clear between their round ends. One item is a whole ring
 * either way. */
export function usageArcs(
  style: RingStyle,
  count: number,
  spec: RingSpec,
): UsageArc[] {
  const n = Math.max(0, Math.min(MAX_USAGE_ITEMS, Math.floor(count)));
  if (!n) return [];
  const half = spec.size / 2;
  if (style === "rings") {
    const band = Math.max(0, (spec.size - spec.hole) / 2);
    const width = Math.min(spec.width, band / (n + (n - 1) * 0.5));
    return Array.from({ length: n }, (_, k) => ({
      radius: half - width / 2 - k * width * 1.5,
      width,
      start: 0,
      end: 360,
    }));
  }
  const width = spec.width,
    radius = half - width / 2;
  if (n === 1) return [{ radius, width, start: 0, end: 360 }];
  const inset = ((spec.gap + width) / 2 / radius) * (180 / Math.PI);
  return SPANS[n].map(([start, end]) => ({
    radius,
    width,
    start: start + inset,
    end: end - inset,
  }));
}

/** The SVG path of `arc` from `start` to `end` round a box's `center`; a
 * whole turn closes on itself. Empty for a span too short to draw. */
export function arcPath(
  center: number,
  arc: Pick<UsageArc, "radius">,
  start: number,
  end: number,
): string {
  const span = end - start,
    r = arc.radius;
  if (span < 0.05 || r <= 0) return "";
  const at = (degrees: number) => {
    const t = (degrees * Math.PI) / 180;
    return `${round(center + r * Math.sin(t))} ${round(center - r * Math.cos(t))}`;
  };
  if (span >= 359.95)
    return `M${at(start)}A${round(r)} ${round(r)} 0 1 1 ${at(start + 180)}A${round(r)} ${round(r)} 0 1 1 ${at(start)}Z`;
  return `M${at(start)}A${round(r)} ${round(r)} 0 ${span > 180 ? 1 : 0} 1 ${at(end)}`;
}

/** Whether an arc closes into a whole ring, which draws without round ends. */
export function isWholeRing(arc: Pick<UsageArc, "start" | "end">): boolean {
  return arc.end - arc.start >= 359.9;
}

const round = (n: number) => Math.round(n * 1000) / 1000;

/** A Simple mark's ring box at `size`: the ring is the mark, its rings stay
 * outside the 55% a logo takes, and two segments are 1.5 pt apart at 22 pt. */
export function simpleRingSpec(size: number, width: number): RingSpec {
  return { size, width, hole: size * 0.55, gap: Math.max(1, size * 0.068) };
}

/** The logo a Simple mark keeps inside `arcs`: 55% of the mark, smaller where
 * the innermost ring comes closer, with a twenty-second of the mark clear. */
export function simpleLogoSize(size: number, arcs: UsageArc[]): number {
  const inner = arcs.at(-1);
  if (!inner) return size * 0.55;
  return Math.min(
    size * 0.55,
    2 * (inner.radius - inner.width / 2 - size / 22),
  );
}
