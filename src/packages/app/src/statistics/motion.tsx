import { useEffect, useRef, useState } from "react";

/** How long a figure takes to reach its new value, ms. */
export const TWEEN_MS = 480;

const ease = (t: number) => 1 - Math.pow(1 - t, 3);
const still = () =>
  typeof matchMedia === "function" &&
  matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Numbers that move to new values rather than jump: from the last value
 * shown (zero at first, so data grows in), eased, over `duration`. With
 * Reduce Motion they change at once. A list of another length grows in
 * from zero. */
export function useTweens(
  targets: readonly number[],
  duration = TWEEN_MS,
  /** False until the values are real; the first real ones then show at
   * once (something else brings them in) and only later changes move. */
  ready = true,
) {
  const settled = useRef(ready);
  const [shown, setShown] = useState<number[]>(() => targets.map(() => 0));
  const current = useRef<number[]>(shown);
  const key = targets.join(",");
  useEffect(() => {
    const from = current.current;
    const to = [...targets];
    if (!ready || !settled.current) {
      settled.current = ready;
      if (!ready && !from.every((value) => value === 0)) return;
      current.current = to;
      setShown(to);
      return;
    }
    // A list of another length (a new range, a new set) grows from zero.
    const start = from.length === to.length ? from : to.map(() => 0);
    if (still() || start.every((value, index) => value === to[index])) {
      current.current = to;
      setShown(to);
      return;
    }
    const began = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const t = ease(Math.min(1, Math.max(0, (now - began) / duration)));
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
  }, [key, duration, ready]);
  return shown;
}

/** One number that moves to new values; see `useTweens`. */
export function useTween(target: number, duration = TWEEN_MS): number {
  return useTweens([target], duration)[0] ?? target;
}

/** A figure written by `format` as its value moves. */
export function Tween({
  value,
  format,
}: {
  value: number;
  format: (value: number) => string;
}) {
  return <>{format(useTween(value))}</>;
}
