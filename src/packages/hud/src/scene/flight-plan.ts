// A host's flight between two agents' marks, planned without a DOM: which one
// is in the air and how many ride along, the route it takes, and every
// keyframe of its story (an envelope packed beside the sender, folded into a
// paper plane, flown, unfolded beside the receiver and delivered), handed to
// Web Animations whole. Nothing here runs per frame.
import type { HUDEdge } from "../model";

/** One flight the host asked for (the host's `Flight`). */
export interface HUDFlight {
  /** Flights between the same two ends share a lane. */
  lane: string;
  from: string;
  to: string;
  /** Around the notch, the sender stands on the left; otherwise on the right. */
  fromLeft: boolean;
  /** The receiver appears first, then catches it. */
  arrives: boolean;
  /** It falls halfway and the sender is left puzzled. */
  falls: boolean;
}
/** What the host sends each HUD: a flight, its arrival order and when it
 * came (`performance.now()`): a scene that mounts later does not replay it. */
export interface HUDFlightCommand {
  flight: HUDFlight;
  sequence: number;
  at: number;
}
/** How old a flight may be when its scene first sees it. */
export const FLIGHT_FRESHNESS_MS = 2000;
/** The flight in the air and how many it carries (one, or more that came meanwhile). */
export interface Ride {
  flight: HUDFlight;
  count: number;
  id: number;
}
export interface Point {
  x: number;
  y: number;
}
/** One cubic Bézier: start, two controls, end. */
export type Cubic = readonly [Point, Point, Point, Point];

const sameRide = (a: HUDFlight, b: HUDFlight) =>
  a.lane === b.lane &&
  a.from === b.from &&
  a.to === b.to &&
  a.falls === b.falls &&
  a.arrives === b.arrives;

/** One flight in the air at a time. A flight like the one in the air (same
 * lane, same way, same ending) rides along as its count; another waits its
 * turn, and more like it ride along with the one waiting. */
export class FlightQueue {
  private next = 1;
  current: Ride | null = null;
  readonly waiting: Ride[] = [];
  /** Takes a flight; true when what is drawn changed (a start or a count). */
  add(flight: HUDFlight): boolean {
    if (this.current && sameRide(this.current.flight, flight)) {
      this.current = { ...this.current, count: this.current.count + 1 };
      return true;
    }
    const waiting = this.waiting.find((ride) => sameRide(ride.flight, flight));
    if (waiting) {
      waiting.count += 1;
      return false;
    }
    const ride = { flight, count: 1, id: this.next++ };
    if (this.current) {
      this.waiting.push(ride);
      return false;
    }
    this.current = ride;
    return true;
  }
  /** The flight in the air landed: the next waiting one takes off. */
  finish(id: number): boolean {
    if (this.current?.id !== id) return false;
    this.current = this.waiting.shift() ?? null;
    return true;
  }
}

/** The story's steps, in milliseconds. */
export const STORY = {
  /** Around the notch, the robots come out first. */
  enter: 320,
  /** An open envelope pops up beside the sender; a letter slides in. */
  pack: 500,
  /** The flap closes and the envelope folds into a paper plane. */
  fold: 600,
  /** In the air, a dashed trail behind it. */
  fly: 1500,
  /** Beside the receiver it unfolds into the envelope and opens. */
  unfold: 600,
  /** The letter slides into the receiver, which bounces; the envelope fades. */
  deliver: 600,
  /** A refused one drops from halfway and bursts open. */
  fall: 700,
  /** Around the notch, the robots go back in. */
  leave: 300,
  /** Reduced motion: an envelope fades in by each side in turn. */
  still: 1800,
} as const;

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

/** CSS ease-in-out (cubic-bezier(0.42, 0, 0.58, 1)) as a function of time. */
export function easeInOut(time: number): number {
  const curve = (s: number, p1: number, p2: number) =>
    3 * (1 - s) * (1 - s) * s * p1 + 3 * (1 - s) * s * s * p2 + s * s * s;
  let low = 0;
  let high = 1;
  for (let index = 0; index < 30; index++) {
    const s = (low + high) / 2;
    if (curve(s, 0.42, 0.58) < time) low = s;
    else high = s;
  }
  return curve((low + high) / 2, 0, 1);
}

/** The time at which ease-in-out has gone `progress` of the way. */
export function easeInOutInverse(progress: number): number {
  let low = 0;
  let high = 1;
  for (let index = 0; index < 30; index++) {
    const time = (low + high) / 2;
    if (easeInOut(time) < progress) low = time;
    else high = time;
  }
  return (low + high) / 2;
}

const round = (value: number) => Math.round(value * 10) / 10;
const fix = (value: number) => Math.round(value * 10000) / 10000;
const px = (point: Point) => `${round(point.x)}px ${round(point.y)}px`;
/** An angle turned by whole turns to be nearest `near`, so a rotation never
 * takes the long way round. */
const unwrap = (angle: number, near: number) =>
  angle + 360 * Math.round((near - angle) / 360);

/** The paper plane, nose to the right, in percent of the craft's box: the
 * classic one seen from three quarters, as a line drawing. Its outline, from
 * the fold where the wings meet: the upper wing's tail, the nose, the lower
 * wing's tail, and the keel's two corners. */
const PLANE_OUTLINE = [
  [22.5, 37.8],
  [0, 0],
  [100, 50],
  [23.8, 100],
  // On the straight line from the fold to the lower wing's tail.
  [23.3, 77.1],
  [4, 76.3],
] as const;
/** The envelope and the plane as clip paths with the same six points, so one
 * can fold into the other; each envelope corner goes to the plane's point
 * nearest it, so the outline never crosses itself on the way. */
export const ENVELOPE =
  "polygon(0% 0%, 50% 0%, 100% 0%, 100% 100%, 50% 100%, 0% 100%)";
export const PLANE = `polygon(${PLANE_OUTLINE.map(([x, y]) => `${x}% ${y}%`).join(", ")})`;
/** The plane as an SVG drawing (in a 100 by 100 box): its outline, and the
 * creases from the fold to the nose and down to the keel. */
export const PLANE_POINTS = PLANE_OUTLINE.map(([x, y]) => `${x},${y}`).join(
  " ",
);
export const PLANE_CREASES = "M22.5 37.8L100 50M22.5 37.8L23.3 77.1";

/** Every keyframe list of one flight's story, all on one clock (`length` ms). */
export interface Story {
  length: number;
  /** When the plane takes off and when it lands or falls, for marks and robots. */
  takeoff: number;
  landing: number;
  /** The craft: where it is, which way it points, its outline, size and opacity. */
  move: Keyframe[];
  turn: Keyframe[];
  /** Drawn mirrored (setting off leftwards) or not, for the whole flight. */
  mirror: Keyframe[];
  shape: Keyframe[];
  pop: Keyframe[];
  /** The envelope's flap (open above, closed over the body, gone while it flies). */
  flap: Keyframe[];
  /** The plane's line drawing, shown while it flies; the folding paper under
   * it shows the rest of the time (one is the other's opposite). */
  folds: Keyframe[];
  /** The envelope's seams, gone while it folds into a plane and back. */
  pocket: Keyframe[];
  /** The letter: into the envelope beside the sender, out into the receiver. */
  letter: Keyframe[];
  /** The trail: short dashes along the route, each shown as the plane passes and fading after. */
  dashes: { translate: string; rotate: string; frames: Keyframe[] }[];
}

/** A flight's whole story. `start` and `end` are beside the sender's and the
 * receiver's marks (where the envelope sits), `from` and `to` the marks
 * themselves (where the letter comes from and goes to). */
export function story({
  route,
  from,
  to,
  falls,
  delay = 0,
}: {
  route: readonly Cubic[];
  from: Point;
  to: Point;
  falls: boolean;
  /** The robots' entrance around the notch. */
  delay?: number;
}): Story {
  const samples = 28;
  const measured = measure(route);
  const start = measured.at(0);
  const finish = measured.at(1);
  const packed = delay + STORY.pack;
  const takeoff = packed + STORY.fold;
  const flown = falls ? STORY.fly * 0.55 : STORY.fly;
  const landing = takeoff + flown;
  const unfolded = landing + STORY.unfold;
  const length = falls ? landing + STORY.fall : unfolded + STORY.deliver;
  const o = (time: number) => fix(time / length);
  const takeoffAngle = unwrap(start.angle, 0);
  // A plane setting off leftwards is drawn mirrored, so the envelope folds
  // straight into it instead of spinning round first.
  const leftward = (angle: number) => Math.cos((angle * Math.PI) / 180) < 0;
  const mirrored = leftward(takeoffAngle);
  // How far the drawing is turned for a heading: the heading itself, or half
  // a turn less when mirrored, kept nearest to no turn at take-off.
  const lift = unwrap(takeoffAngle - (mirrored ? 180 : 0), 0) - takeoffAngle;
  const shown = (angle: number) => angle + lift;

  // In the air: time-even samples eased in and out, so the speed follows the
  // easing while every sample is placed by length along the route.
  const air: { offset: number; point: Point; angle: number }[] = [];
  let last = takeoffAngle;
  const reach = falls ? 0.5 : 1;
  for (let index = 0; index <= samples; index++) {
    const time = index / samples;
    const { point, angle } = measured.at(easeInOut(time) * reach);
    last = unwrap(angle, last);
    air.push({ offset: o(takeoff + flown * time), point, angle: last });
  }
  const rest = air[air.length - 1];
  const move: Keyframe[] = [
    { offset: 0, translate: px(start.point) },
    { offset: o(takeoff), translate: px(start.point) },
    ...air
      .slice(1)
      .map(({ offset, point }) => ({ offset, translate: px(point) })),
  ];
  const turn: Keyframe[] = [
    { offset: 0, rotate: "0deg" },
    { offset: o(packed), rotate: "0deg" },
    {
      offset: o(takeoff),
      rotate: `${round(shown(takeoffAngle))}deg`,
    },
    ...air.slice(1).map(({ offset, angle }) => ({
      offset,
      rotate: `${round(shown(angle))}deg`,
    })),
  ];
  const flip = (mirror: boolean) => (mirror ? "-1 1" : "1 1");
  const mirror: Keyframe[] = [
    { offset: 0, scale: flip(mirrored) },
    { offset: o(landing), scale: flip(mirrored) },
  ];
  const shape: Keyframe[] = [
    { offset: 0, clipPath: ENVELOPE },
    { offset: o(packed + STORY.fold * 0.3), clipPath: ENVELOPE },
    { offset: o(takeoff), clipPath: PLANE },
  ];
  const flap: Keyframe[] = [
    { offset: 0, scale: "1 -1", opacity: 1 },
    { offset: o(packed - 80), scale: "1 -1", opacity: 1 },
    { offset: o(packed + 120), scale: "1 1", opacity: 1 },
    { offset: o(packed + STORY.fold * 0.55), scale: "1 1", opacity: 0 },
  ];
  // The folding paper and the drawn plane hand over in 0.1 s once the paper
  // has the plane's outline, and back again before it unfolds.
  const handover = 100;
  const folds: Keyframe[] = [
    { offset: 0, opacity: 0 },
    { offset: o(takeoff - handover), opacity: 0 },
    { offset: o(takeoff), opacity: 1 },
  ];
  const pocket: Keyframe[] = [
    { offset: 0, opacity: 1 },
    { offset: o(packed + STORY.fold * 0.2), opacity: 1 },
    { offset: o(packed + STORY.fold * 0.5), opacity: 0 },
  ];
  const pop: Keyframe[] = [
    { offset: 0, scale: "0.3", opacity: 0 },
    { offset: o(delay + 220), scale: "1.08", opacity: 1 },
    { offset: o(delay + 320), scale: "1", opacity: 1 },
  ];
  const letter: Keyframe[] = [
    { offset: 0, translate: px(from), scale: "0.4", opacity: 0 },
    { offset: o(delay + 120), translate: px(from), scale: "0.55", opacity: 1 },
    {
      offset: o(packed - 40),
      translate: px(start.point),
      scale: "0.8",
      opacity: 1,
    },
    { offset: o(packed), translate: px(start.point), scale: "0.8", opacity: 0 },
  ];
  // A dash every 12 points of the way flown, behind the plane: shown as it
  // passes (the easing inverted: when the plane is there), gone 0.6 s later.
  const dashes: Story["dashes"] = [];
  const count = Math.min(40, Math.floor((measured.length * reach) / 12));
  for (let index = 1; index < count; index++) {
    const fraction = (index / count) * reach;
    const { point, angle } = measured.at(fraction);
    const passes = takeoff + flown * easeInOutInverse(fraction / reach);
    dashes.push({
      translate: px(point),
      rotate: `${round(angle)}deg`,
      frames: [
        { offset: 0, opacity: 0 },
        { offset: o(passes), opacity: 0 },
        { offset: o(Math.min(length, passes + 60)), opacity: 0.8 },
        { offset: o(Math.min(length, passes + 650)), opacity: 0 },
        { offset: 1, opacity: 0 },
      ],
    });
  }
  if (falls) {
    // It noses down from halfway, bursts back into an open envelope and fades.
    const drop = (dx: number, dy: number) => ({
      x: rest.point.x + dx,
      y: rest.point.y + dy,
    });
    for (const [time, dx, dy, spin] of [
      [0.3, 6, 14, 50],
      [0.65, 4, 34, 110],
      [1, 0, 52, 150],
    ] as const) {
      move.push({
        offset: o(landing + STORY.fall * time),
        translate: px(drop(dx, dy)),
      });
      turn.push({
        offset: o(landing + STORY.fall * time),
        rotate: `${round(shown(rest.angle) + spin)}deg`,
      });
    }
    shape.push(
      { offset: o(landing + STORY.fall * 0.25 + handover), clipPath: PLANE },
      { offset: o(landing + STORY.fall * 0.7), clipPath: ENVELOPE },
      { offset: 1, clipPath: ENVELOPE },
    );
    flap.push(
      { offset: o(landing + STORY.fall * 0.5), scale: "1 1", opacity: 0 },
      { offset: o(landing + STORY.fall * 0.7), scale: "1 -1", opacity: 1 },
      { offset: 1, scale: "1 -1", opacity: 1 },
    );
    folds.push(
      { offset: o(landing + STORY.fall * 0.25), opacity: 1 },
      { offset: o(landing + STORY.fall * 0.25 + handover), opacity: 0 },
      { offset: 1, opacity: 0 },
    );
    pocket.push(
      { offset: o(landing + STORY.fall * 0.6), opacity: 0 },
      { offset: o(landing + STORY.fall * 0.8), opacity: 1 },
      { offset: 1, opacity: 1 },
    );
    pop.push(
      { offset: o(landing + STORY.fall * 0.5), scale: "1", opacity: 1 },
      { offset: 1, scale: "0.9", opacity: 0 },
    );
  } else {
    // The keel stays under the wing for the whole flight (upside down at
    // the top of a loop, as a real one is), and the plane turns upright as it
    // unfolds.
    const endAngle = unwrap(0, shown(rest.angle));
    move.push({ offset: 1, translate: px(finish.point) });
    turn.push(
      {
        offset: o(landing + STORY.unfold * 0.5),
        rotate: `${round(endAngle)}deg`,
      },
      { offset: 1, rotate: `${round(endAngle)}deg` },
    );
    shape.push(
      { offset: o(landing + handover), clipPath: PLANE },
      { offset: o(landing + STORY.unfold * 0.6), clipPath: ENVELOPE },
      { offset: 1, clipPath: ENVELOPE },
    );
    flap.push(
      { offset: o(landing + STORY.unfold * 0.45), scale: "1 1", opacity: 0 },
      { offset: o(landing + STORY.unfold * 0.7), scale: "1 1", opacity: 1 },
      { offset: o(unfolded), scale: "1 -1", opacity: 1 },
      { offset: 1, scale: "1 -1", opacity: 1 },
    );
    folds.push(
      { offset: o(landing), opacity: 1 },
      { offset: o(landing + handover), opacity: 0 },
      { offset: 1, opacity: 0 },
    );
    pocket.push(
      { offset: o(landing + STORY.unfold * 0.5), opacity: 0 },
      { offset: o(landing + STORY.unfold * 0.8), opacity: 1 },
      { offset: 1, opacity: 1 },
    );
    pop.push(
      { offset: o(unfolded + STORY.deliver * 0.6), scale: "1", opacity: 1 },
      { offset: 1, scale: "0.85", opacity: 0 },
    );
    letter.push(
      {
        offset: o(unfolded),
        translate: px(finish.point),
        scale: "0.8",
        opacity: 0,
      },
      {
        offset: o(unfolded + 60),
        translate: px(finish.point),
        scale: "0.8",
        opacity: 1,
      },
      {
        offset: o(unfolded + STORY.deliver * 0.75),
        translate: px(to),
        scale: "0.45",
        opacity: 1,
      },
      { offset: 1, translate: px(to), scale: "0.3", opacity: 0 },
    );
  }
  if (falls)
    letter.push({
      offset: 1,
      translate: px(start.point),
      scale: "0.8",
      opacity: 0,
    });
  mirror.push({ offset: 1, scale: mirror[mirror.length - 1].scale });
  return {
    length,
    takeoff,
    landing,
    move,
    turn,
    mirror,
    shape,
    pop,
    flap,
    folds,
    pocket,
    letter,
    dashes,
  };
}
/** Reduced motion: no flight. A closed envelope fades in beside the sender and
 * out again, then an open one beside the receiver; a refused one only shows
 * the sender's, open. Keyframes for the craft's two showings (`first`, `second`). */
export function stillStory(falls: boolean): {
  first: Keyframe[];
  second: Keyframe[];
} {
  const show = (from: number, to: number): Keyframe[] => [
    { offset: 0, opacity: 0 },
    { offset: from, opacity: 0 },
    { offset: from + 0.08, opacity: 1 },
    { offset: to - 0.08, opacity: 1 },
    { offset: to, opacity: 0 },
    { offset: 1, opacity: 0 },
  ];
  return falls
    ? { first: show(0, 0.6), second: [{ opacity: 0 }, { opacity: 0 }] }
    : { first: show(0, 0.5), second: show(0.45, 0.95) };
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
