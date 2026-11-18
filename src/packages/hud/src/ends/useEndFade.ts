import { useEffect, useRef, useState } from "react";

/** A bubble's countdown at the dock's end, the same for every end that
 * fades. It runs `ms` from when it begins, and again whenever `begins`
 * changes (a new copy). The pointer on the bubble (`paused`) stops it and
 * winds it back, so the bubble has its whole time again once the pointer
 * leaves. When it runs out it calls `done`; with `repeat` the next round
 * starts (the tail lets its oldest message go each round). Not `active`, or
 * an `ms` of 0, and nothing runs. `cycle` changes whenever a round starts,
 * for the ring that shows it. */
export function useEndFade({
  active,
  begins = null,
  paused,
  ms,
  done,
  repeat = false,
}: {
  active: boolean;
  begins?: string | null;
  paused: boolean;
  ms: number;
  done: () => void;
  repeat?: boolean;
}) {
  const [cycle, setCycle] = useState(0);
  const left = useRef(ms);
  const finish = useRef(done);
  finish.current = done;
  const running = active && ms > 0;
  useEffect(() => {
    if (!running) return;
    left.current = ms;
    setCycle((n) => n + 1);
  }, [running, ms, begins, paused]);
  useEffect(() => {
    if (!running || paused) return;
    const start = performance.now();
    let fired = false;
    const timer = setTimeout(() => {
      fired = true;
      finish.current();
      if (repeat) {
        left.current = ms;
        setCycle((n) => n + 1);
      }
    }, left.current);
    return () => {
      clearTimeout(timer);
      if (!fired)
        left.current = Math.max(0, left.current - (performance.now() - start));
    };
  }, [running, paused, cycle]);
  return { cycle, paused: paused || !running, ms: running ? ms : 0 };
}

export type EndFade = ReturnType<typeof useEndFade>;
