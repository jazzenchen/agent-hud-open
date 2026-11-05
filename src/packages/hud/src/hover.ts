import { useCallback, useEffect, useRef, useState } from "react";

export type HoverState =
  | { phase: "collapsed" | "open" }
  | { phase: "opening" | "closing"; deadline: number };
export type HoverEvent =
  | { type: "enter" | "leave" | "timer"; now: number }
  | { type: "force-open" | "force-collapse" };
export interface HoverTiming {
  hoverDelayMs: number;
  collapseDelayMs: number;
}

/** Direct port of HoverMachine.swift; all deadlines are monotonic browser times. */
export function reduceHover(
  state: HoverState,
  event: HoverEvent,
  config: HoverTiming,
): HoverState {
  if (event.type === "force-open") return { phase: "open" };
  if (event.type === "force-collapse") return { phase: "collapsed" };
  if (state.phase === "collapsed" && event.type === "enter")
    return { phase: "opening", deadline: event.now + config.hoverDelayMs };
  if (state.phase === "opening" && event.type === "leave")
    return { phase: "collapsed" };
  if (
    state.phase === "opening" &&
    event.type === "timer" &&
    event.now >= state.deadline
  )
    return { phase: "open" };
  if (state.phase === "open" && event.type === "leave")
    return { phase: "closing", deadline: event.now + config.collapseDelayMs };
  if (state.phase === "closing" && event.type === "enter")
    return { phase: "open" };
  if (
    state.phase === "closing" &&
    event.type === "timer" &&
    event.now >= state.deadline
  )
    return { phase: "collapsed" };
  return state;
}

export function useHoverMachine(config: HoverTiming) {
  const [state, setState] = useState<HoverState>({ phase: "collapsed" });
  const inside = useRef(false);
  const send = useCallback(
    (event: HoverEvent) =>
      setState((current) => reduceHover(current, event, config)),
    [config.hoverDelayMs, config.collapseDelayMs],
  );
  useEffect(() => {
    if (state.phase !== "opening" && state.phase !== "closing") return;
    // A timer's delay is cut to whole milliseconds and can fire a moment
    // before the deadline; it was set for the deadline, so it has come.
    const deadline = state.deadline;
    const timer = setTimeout(
      () =>
        send({
          type: "timer",
          now: Math.max(performance.now(), deadline),
        }),
      Math.max(1, Math.ceil(deadline - performance.now())),
    );
    return () => clearTimeout(timer);
  }, [state, send]);
  const enter = useCallback(() => {
    if (inside.current) return;
    inside.current = true;
    send({ type: "enter", now: performance.now() });
  }, [send]);
  const leave = useCallback(() => {
    if (!inside.current) return;
    inside.current = false;
    send({ type: "leave", now: performance.now() });
  }, [send]);
  const forceOpen = useCallback(() => send({ type: "force-open" }), [send]);
  const collapse = useCallback(() => {
    inside.current = false;
    send({ type: "force-collapse" });
  }, [send]);
  /** An open panel the pointer is not on closes after the collapse delay;
   * one under the pointer stays (ScreenHUD+Hold's end of a hold). */
  const settle = useCallback(() => {
    if (!inside.current) send({ type: "leave", now: performance.now() });
  }, [send]);
  return {
    state,
    open: state.phase === "open" || state.phase === "closing",
    enter,
    leave,
    forceOpen,
    collapse,
    settle,
    inside,
  };
}
