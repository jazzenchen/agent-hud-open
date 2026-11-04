import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import { panWindow } from "../lib/session-charts";

/** A stretch of the turn, ms. */
export type View = [number, number];

/** How quickly the view glides after a zoom and after a pan: ms to cover about two thirds of the way. */
const GLIDE = 70;
const PAN_GLIDE = 40;

/**
 * The stretch of a turn a timeline's plot shows, and the glide between
 * stretches. `view` is the window drawn, `aim` the one it glides to; null is
 * the whole turn, from `start` to `fullTo`. Swiping sideways on a trackpad, or
 * shift with a mouse wheel, pans a zoomed view.
 */
export function useTimelineGlide(
  plot: RefObject<HTMLDivElement | null>,
  start: number,
  fullTo: number,
) {
  const [view, setView] = useState<View | null>(null);
  const [aim, setAim] = useState<View | null>(null);

  const [plotWidth, setPlotWidth] = useState(0);
  useLayoutEffect(() => {
    const node = plot.current;
    if (!node) return;
    setPlotWidth(node.clientWidth);
    const observer = new ResizeObserver(([entry]) =>
      setPlotWidth(entry.contentRect.width),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [plot]);

  // What the glide and the wheel read between renders.
  const glide = useRef({
    view: null as View | null,
    aim: null as View | null,
    frame: 0,
    tau: GLIDE,
    start,
    fullTo,
    plotWidth,
  });
  useLayoutEffect(() => {
    Object.assign(glide.current, { start, fullTo, plotWidth });
  });
  useEffect(() => () => cancelAnimationFrame(glide.current.frame), []);
  const glideTo = useCallback((next: View | null, tau = GLIDE) => {
    const g = glide.current;
    g.aim = next;
    g.tau = tau;
    setAim(next);
    if (g.frame) return;
    // A frame's time can come before the moment the glide began, so the
    // first frame counts as one frame (16 ms), never as a step backwards.
    let last = 0;
    const step = (time: number) => {
      const whole: View = [g.start, g.fullTo];
      const goal = g.aim ?? whole;
      const [a, b] = g.view ?? whole;
      const elapsed = last ? Math.min(Math.max(0, time - last), 64) : 16;
      const k = 1 - Math.exp(-elapsed / g.tau);
      last = time;
      const moved: View = [a + (goal[0] - a) * k, b + (goal[1] - b) * k];
      const pixels =
        Math.max(Math.abs(goal[0] - moved[0]), Math.abs(goal[1] - moved[1])) *
        (g.plotWidth / Math.min(goal[1] - goal[0], moved[1] - moved[0]));
      const done = pixels < 0.25;
      g.view = done ? g.aim : moved;
      g.frame = done ? 0 : requestAnimationFrame(step);
      setView(g.view);
    };
    g.frame = requestAnimationFrame(step);
  }, []);
  // Up and down still scroll the page. React's wheel listener is passive, so
  // this one is added by hand to keep the page from moving too.
  useEffect(() => {
    const node = plot.current;
    if (!node) return;
    const onWheel = (event: WheelEvent) => {
      const g = glide.current;
      const sideways =
        event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY);
      if (!sideways || !g.aim) return;
      event.preventDefault();
      const delta = event.deltaX || event.deltaY;
      const pixels =
        delta *
        (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? g.plotWidth : 1);
      const length = g.aim[1] - g.aim[0];
      glideTo(
        panWindow(
          g.aim,
          (pixels / Math.max(1, g.plotWidth)) * length,
          g.start,
          g.fullTo,
        ),
        PAN_GLIDE,
      );
    };
    node.addEventListener("wheel", onWheel, { passive: false });
    return () => node.removeEventListener("wheel", onWheel);
  }, [plot, glideTo]);

  return { view, aim, plotWidth, glideTo };
}
