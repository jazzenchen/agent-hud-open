import { useEffect, useLayoutEffect, useRef } from "react";
import { startGpuPaint, type GpuPaint } from "./glow-gpu";
import { RIM_COLORS, RIM_NUMBERS, RIM_PAINT } from "./glow-rim-shader";
import type { RimShape, RimTrack } from "./rim-track";

type RimValues = Record<string, number | string>;

/** The silhouette's part of the rim's values. */
function shapeValues(shape: RimShape): RimValues {
  return {
    stripWidth: shape.width,
    stripHeight: shape.height,
    corner: shape.radius,
    shoulder: shape.flare,
    offsetX: shape.x,
    offsetY: shape.y,
  };
}

/**
 * The rim on a WebGPU canvas of its own while it is shown (one a rim before it
 * gave back, when there is one), drawing only when asked: a change of size or
 * value draws one frame, and a moving pattern advances `clock` at
 * `framesPerSecond`.
 */
export function GpuRim({
  values,
  track,
  framesPerSecond,
  shown,
  onReady,
  onUnavailable,
}: {
  values: RimValues;
  track: RimTrack;
  framesPerSecond: number;
  shown: boolean;
  onReady: () => void;
  onUnavailable: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const live = useRef<GpuPaint | null>(null);
  const latest = useRef(values);
  latest.current = values;
  const latestTrack = useRef(track);
  latestTrack.current = track;
  const callbacks = useRef({ onReady, onUnavailable });
  callbacks.current = { onReady, onUnavailable };
  const start = useRef(0);
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    start.current = performance.now();
    const paint = startGpuPaint({
      host: element,
      numbers: RIM_NUMBERS,
      colors: RIM_COLORS,
      paint: RIM_PAINT,
      values: {
        ...latest.current,
        ...shapeValues(latestTrack.current.get()),
        clock: 0,
      },
      onReady: () => callbacks.current.onReady(),
      onUnavailable: () => callbacks.current.onUnavailable(),
    });
    live.current = paint;
    return () => {
      live.current = null;
      paint.dispose();
    };
  }, []);
  useLayoutEffect(() => {
    live.current?.update({ ...values, ...shapeValues(track.get()) });
  });
  // The silhouette moves in a frame of its own animation: draw in that
  // frame, not the next one, so the rim never trails it.
  useLayoutEffect(
    () =>
      track.subscribe(() =>
        live.current?.update(shapeValues(track.get()), true),
      ),
    [track],
  );
  useEffect(() => {
    if (framesPerSecond <= 0) return;
    const timer = setInterval(
      () =>
        live.current?.update({
          clock: (performance.now() - start.current) / 1000,
        }),
      1000 / framesPerSecond,
    );
    return () => clearInterval(timer);
  }, [framesPerSecond]);
  return (
    <div
      ref={host}
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        opacity: shown ? 1 : 0,
      }}
    />
  );
}
