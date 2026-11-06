import { useEffect, useRef, useState } from "react";
import { TWEEN_MS } from "../statistics/motion";

const ease = (t: number) => 1 - Math.pow(1 - t, 3);
const still = () =>
  typeof matchMedia === "function" &&
  matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * A chart's values easing to new ones, as `useTweens` moves figures: from
 * `first` the first time they are drawn (zero unless given), then from what
 * was drawn. A value added at the end starts where the last one stood, so a
 * new turn carries the line on rather than rising from nothing; a value
 * taken away goes at once. With Reduce Motion they change at once.
 */
export function useGlide(
  targets: readonly number[],
  first?: readonly number[],
): number[] {
  const [shown, setShown] = useState<number[]>(() => [
    ...(first ?? targets.map(() => 0)),
  ]);
  const current = useRef(shown);
  const key = targets.join(",");
  useEffect(() => {
    const before = current.current;
    const to = [...targets];
    const start = to.map((_, index) => before[index] ?? before.at(-1) ?? 0);
    if (still() || start.every((value, index) => value === to[index])) {
      current.current = to;
      setShown(to);
      return;
    }
    const began = performance.now();
    let frame = 0;
    const step = (now: number) => {
      // A frame's time can come before the move began: never step back.
      const t = ease(Math.min(1, Math.max(0, (now - began) / TWEEN_MS)));
      const next = to.map(
        (value, index) => start[index] + (value - start[index]) * t,
      );
      current.current = next;
      setShown(next);
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
    // The targets' values are the key; the array itself changes every render.
  }, [key]);
  // Until the move starts, an added value stands where the last one was.
  return targets.map((target, index) => shown[index] ?? shown.at(-1) ?? target);
}
