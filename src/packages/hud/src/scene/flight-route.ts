// A flight's route, planned without a DOM: part of an ellipse from beside the
// sender to beside the receiver, measured along its length, and around the
// notch, where its robots stand.
import type { HUDEdge } from "../model";

export interface Point {
  x: number;
  y: number;
}
/** One cubic Bézier: start, two controls, end. */
export type Cubic = readonly [Point, Point, Point, Point];

/** The direction away from a screen edge, into the screen. */
export const inward: Record<HUDEdge, Point> = {
  top: { x: 0, y: 1 },
  bottom: { x: 0, y: -1 },
  left: { x: 1, y: 0 },
  right: { x: -1, y: 0 },
};

const add = (a: Point, b: Point): Point => ({ x: a.x + b.x, y: a.y + b.y });
const sub = (a: Point, b: Point): Point => ({ x: a.x - b.x, y: a.y - b.y });
const mul = (a: Point, k: number): Point => ({ x: a.x * k, y: a.y * k });
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);

/** Part of an ellipse as cubic Béziers, one per at most 72 degrees: the
 * points `centre + n·a·cos θ + u·b·sin θ` for θ from `from` to `to`. A cubic
 * with handles 4/3·tan(Δ/4) long matches a circle's arc to within a few
 * hundredths of a percent, and an ellipse is a stretched circle. */
function ellipse(
  centre: Point,
  n: Point,
  u: Point,
  a: number,
  b: number,
  from: number,
  to: number,
): Cubic[] {
  const at = (angle: number) =>
    add(centre, add(mul(n, a * Math.cos(angle)), mul(u, b * Math.sin(angle))));
  const along = (angle: number) =>
    add(mul(n, -a * Math.sin(angle)), mul(u, b * Math.cos(angle)));
  const count = Math.max(
    1,
    Math.ceil(Math.abs(to - from) / ((72 * Math.PI) / 180)),
  );
  const step = (to - from) / count;
  const handle = (4 / 3) * Math.tan(step / 4);
  const segments: Cubic[] = [];
  for (let index = 0; index < count; index++) {
    const first = from + step * index;
    const last = from + step * (index + 1);
    // Each curve starts exactly where the one before ended.
    const p0 = segments.length ? segments[segments.length - 1][3] : at(first);
    const p3 = at(last);
    segments.push([
      p0,
      add(p0, mul(along(first), handle)),
      sub(p3, mul(along(last), handle)),
      p3,
    ]);
  }
  return segments;
}

/** The route with its ends exactly where they were asked to be (the
 * ellipse's trigonometry leaves them a rounding error off). */
function pin(route: Cubic[], start: Point, end: Point): Cubic[] {
  const last = route.length - 1;
  return route.map(([a, b, c, d], index) => [
    index === 0 ? start : a,
    b,
    c,
    index === last ? end : d,
  ]);
}

/** How far the loop reaches into the screen when there is room for it. */
export const LOOP = 140;

/** The route on a dock from beside one mark to beside another: part of an
 * ellipse through both, reaching `LOOP` points into the screen. Close marks
 * get a round loop (the ellipse is wider than the marks are apart, so the
 * plane leaves away from the receiver and comes back round to it); far ones
 * half an ellipse; from a mark to itself, all the way round. The loop shrinks
 * until it stays `margin` inside `bounds`. */
export function dockRoute(
  start: Point,
  end: Point,
  edge: HUDEdge,
  bounds?: { width: number; height: number },
  margin = 16,
): Cubic[] {
  const n = inward[edge];
  for (let loop = LOOP; ; loop *= 0.85) {
    const route = arc(start, end, n, loop);
    if (!bounds || loop < 48 || fits(route, bounds, margin)) return route;
  }
}
function arc(start: Point, end: Point, n: Point, reach: number): Cubic[] {
  const d = distance(start, end);
  // Along the dock, from the sender towards the receiver.
  const u = d > 1e-6 ? mul(sub(end, start), 1 / d) : { x: -n.y, y: n.x };
  const half = d / 2;
  const middle = mul(add(start, end), 0.5);
  if (d >= 2 * reach) {
    const a = Math.min(120, 0.3 * d);
    return pin(
      ellipse(middle, n, u, a, half, -Math.PI / 2, Math.PI / 2),
      start,
      end,
    );
  }
  // The ellipse is 30 points wider than the marks on each side (and never
  // much narrower than it is deep, so a loop to the same mark stays round); its centre
  // sits inward so that both ends lie on it and its far side is `reach` in.
  const b = Math.max(half + 30, reach * 0.4);
  const k = Math.sqrt(1 - (half / b) ** 2);
  const a = reach / (1 + k);
  const centre = add(middle, mul(n, a * k));
  const side = Math.atan2(half / b, -k);
  return pin(ellipse(centre, n, u, a, b, -side, side), start, end);
}

/** Around the notch: half an ellipse from beside one robot, down to `dip`
 * under the notch's lower edge, and up to beside the other. */
export function notchRoute(start: Point, end: Point, dip: number): Cubic[] {
  const d = distance(start, end);
  const u = d > 1e-6 ? mul(sub(end, start), 1 / d) : { x: 1, y: 0 };
  const middle = mul(add(start, end), 0.5);
  return pin(
    ellipse(
      middle,
      inward.top,
      u,
      Math.max(8, dip - middle.y),
      d / 2,
      -Math.PI / 2,
      Math.PI / 2,
    ),
    start,
    end,
  );
}

const cubicAt = ([a, b, c, d]: Cubic, t: number): Point => {
  const u = 1 - t;
  return {
    x:
      u * u * u * a.x +
      3 * u * u * t * b.x +
      3 * u * t * t * c.x +
      t * t * t * d.x,
    y:
      u * u * u * a.y +
      3 * u * u * t * b.y +
      3 * u * t * t * c.y +
      t * t * t * d.y,
  };
};
const tangentAt = ([a, b, c, d]: Cubic, t: number): Point => {
  const u = 1 - t;
  return {
    x:
      3 * u * u * (b.x - a.x) +
      6 * u * t * (c.x - b.x) +
      3 * t * t * (d.x - c.x),
    y:
      3 * u * u * (b.y - a.y) +
      6 * u * t * (c.y - b.y) +
      3 * t * t * (d.y - c.y),
  };
};
/** The heading of a direction, in degrees (0 is to the right). */
export const heading = (direction: Point) =>
  (Math.atan2(direction.y, direction.x) * 180) / Math.PI;

/** A route measured along its length: where a fraction of it is, and which way it heads. */
export interface Measured {
  length: number;
  at(fraction: number): { point: Point; angle: number };
}
const STEPS = 64;
export function measure(route: readonly Cubic[]): Measured {
  const table: { length: number; segment: number; t: number }[] = [
    { length: 0, segment: 0, t: 0 },
  ];
  let length = 0;
  route.forEach((segment, index) => {
    let previous = segment[0];
    for (let step = 1; step <= STEPS; step++) {
      const t = step / STEPS;
      const point = cubicAt(segment, t);
      length += distance(previous, point);
      previous = point;
      table.push({ length, segment: index, t });
    }
  });
  return {
    length,
    at(fraction) {
      const goal = Math.min(1, Math.max(0, fraction)) * length;
      let low = 0;
      let high = table.length - 1;
      while (high - low > 1) {
        const middle = (low + high) >> 1;
        if (table[middle].length < goal) low = middle;
        else high = middle;
      }
      const a = table[low];
      const b = table[high];
      const share =
        b.length > a.length ? (goal - a.length) / (b.length - a.length) : 0;
      const segment = b.segment;
      const t = (a.segment === b.segment ? a.t : 0) * (1 - share) + b.t * share;
      return {
        point: cubicAt(route[segment], t),
        angle: heading(tangentAt(route[segment], t)),
      };
    },
  };
}

/** True when every point of the route stays `margin` inside `bounds`. */
export function fits(
  route: readonly Cubic[],
  bounds: { width: number; height: number },
  margin: number,
): boolean {
  return route.every((segment) => {
    for (let step = 0; step <= 24; step++) {
      const { x, y } = cubicAt(segment, step / 24);
      if (
        x < margin ||
        y < margin ||
        x > bounds.width - margin ||
        y > bounds.height - margin
      )
        return false;
    }
    return true;
  });
}

/** Where a flight around the notch happens: robots peeking out of small wings
 * at the notch's sides, and the path dipping under the notch's lower edge. */
export interface NotchStage {
  left: Point;
  right: Point;
  /** Each wing: x and width; it grows out from under the notch. A wing is
   * drawn as its side of a wider notch (the same flare at the top, the same
   * rounded corner below) and reaches far enough under the notch to cover
   * the notch's own rounded corner. */
  wings: {
    left: { x: number; width: number };
    right: { x: number; width: number };
  };
  /** The resting notch's flare at the top and its corner radius below. */
  flare: number;
  radius: number;
  top: number;
  height: number;
  bot: number;
  /** The lowest point of the path. */
  dip: number;
}
export function notchStage(notch: {
  x: number;
  y: number;
  width: number;
  height: number;
}): NotchStage {
  const height = Math.max(24, notch.height);
  const bot = Math.round(Math.min(22, height - 6));
  const wing = bot + 18;
  // The resting notch's shape (HUDMarkStrip's silhouette).
  const flare = 8;
  const radius = 12;
  const under = flare + radius * 2 + 4;
  return {
    left: { x: notch.x - wing / 2, y: notch.y + height / 2 },
    right: { x: notch.x + notch.width + wing / 2, y: notch.y + height / 2 },
    wings: {
      left: { x: notch.x - wing - flare, width: flare + wing + under },
      right: { x: notch.x + notch.width - under, width: under + wing + flare },
    },
    flare,
    radius,
    top: notch.y,
    height,
    bot,
    dip: notch.y + height + Math.max(30, height * 1.2),
  };
}
