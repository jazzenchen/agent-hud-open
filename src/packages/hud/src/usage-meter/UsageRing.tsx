import { useEffect, useRef, useState, type CSSProperties } from "react";
import { HUD_METRICS } from "../model";
import {
  arcPath,
  isWholeRing,
  usageArcs,
  type RingSpec,
  type RingStyle,
  type UsageArc,
} from "./arcs";
import { itemShare, type RingItem } from "./mark-windows";

export type { RingItem } from "./mark-windows";

interface Drawn extends UsageArc {
  percent: number;
  opacity: number;
  color: string;
}

/** Concentric rings or a segmented ring for `items`, in a `spec.size` box.
 * A change of values, of style or of how many items there are moves the
 * same arcs to their new place: rings gather into one ring as they shrink to
 * their segment, a new item grows from its start, a removed one shrinks back
 * into it. `highlight` keeps one item bright and dims the others. */
export function UsageRing({
  style,
  items,
  spec,
  track,
  highlight,
  reduceMotion = false,
  className,
  styles,
}: {
  style: RingStyle;
  items: RingItem[];
  spec: RingSpec;
  track: string;
  highlight?: number | null;
  reduceMotion?: boolean;
  className?: string;
  styles?: CSSProperties;
}) {
  const target = usageArcs(style, items.length, spec).map((arc, i): Drawn => ({
    ...arc,
    percent: itemShare(items[i].percent),
    opacity: 1,
    color: items[i].color,
  }));
  const frame = useTween(target, reduceMotion);
  const center = spec.size / 2;
  return (
    <svg
      viewBox={`0 0 ${spec.size} ${spec.size}`}
      width={spec.size}
      height={spec.size}
      aria-hidden="true"
      className={className}
      style={{ overflow: "visible", ...styles }}
    >
      {frame.map((arc, i) => {
        if (arc.opacity <= 0.01) return null;
        const whole = isWholeRing(arc),
          fill = arc.start + ((arc.end - arc.start) * arc.percent) / 100;
        return (
          <g
            key={i}
            style={{
              opacity: highlight == null || highlight === i ? 1 : 0.28,
              transition: reduceMotion ? undefined : "opacity 0.18s",
            }}
          >
            <g opacity={arc.opacity}>
              <path
                d={arcPath(center, arc, arc.start, arc.end)}
                fill="none"
                stroke={track}
                strokeWidth={arc.width}
                strokeLinecap={whole ? "butt" : "round"}
              />
              {arc.percent > 0.3 && (
                <path
                  d={arcPath(center, arc, arc.start, fill)}
                  fill="none"
                  stroke={arc.color}
                  strokeWidth={arc.width}
                  strokeLinecap="round"
                />
              )}
            </g>
          </g>
        );
      })}
    </svg>
  );
}

/** `target` as drawn this frame: the arcs move from where they were to where
 * they go over the HUD's own duration and curve, at once under reduced motion. */
function useTween(target: Drawn[], reduceMotion: boolean): Drawn[] {
  const [frame, setFrame] = useState(target);
  const shown = useRef(target);
  const key = JSON.stringify(target);
  useEffect(() => {
    const from = shown.current;
    if (reduceMotion || JSON.stringify(from) === key) {
      shown.current = target;
      setFrame(target);
      return;
    }
    const pairs = Array.from(
      { length: Math.max(from.length, target.length) },
      (_, i) => {
        const a = from[i],
          b = target[i];
        if (a && b) return { a, b, gone: false };
        if (b)
          return {
            a: { ...b, end: b.start, percent: 0, opacity: 0 },
            b,
            gone: false,
          };
        return {
          a,
          b: { ...a, end: a.start, percent: 0, opacity: 0 },
          gone: true,
        };
      },
    );
    const began = performance.now(),
      duration = HUD_METRICS.duration * 1000;
    let raf = 0;
    const step = (now: number) => {
      const t = Math.min(1, (now - began) / duration),
        e = ease(t);
      const drawn = pairs.map(({ a, b }) => ({
        radius: mix(a.radius, b.radius, e),
        width: mix(a.width, b.width, e),
        start: mix(a.start, b.start, e),
        end: mix(a.end, b.end, e),
        percent: mix(a.percent, b.percent, e),
        opacity: mix(a.opacity, b.opacity, e),
        color: b.color,
      }));
      shown.current = drawn.filter((_, i) => !pairs[i].gone || t < 1);
      setFrame(t < 1 ? drawn : target);
      if (t < 1) raf = requestAnimationFrame(step);
      else shown.current = target;
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
    // `key` stands for `target`, which is a new array each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, reduceMotion]);
  return frame;
}

const mix = (a: number, b: number, t: number) => a + (b - a) * t;

/** HUD_METRICS.ease, the cubic Bézier every HUD transition follows. */
function ease(t: number): number {
  const [x1, y1, x2, y2] = HUD_METRICS.ease;
  const bez = (p1: number, p2: number, s: number) =>
    3 * (1 - s) * (1 - s) * s * p1 + 3 * (1 - s) * s * s * p2 + s * s * s;
  let lo = 0,
    hi = 1,
    s = t;
  for (let i = 0; i < 20; i++) {
    s = (lo + hi) / 2;
    if (bez(x1, x2, s) < t) lo = s;
    else hi = s;
  }
  return bez(y1, y2, s);
}
