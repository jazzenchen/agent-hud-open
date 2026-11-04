import { memo, useLayoutEffect, useMemo, useState } from "react";
import {
  animate,
  motion,
  useMotionValue,
  useMotionValueEvent,
  useTransform,
} from "motion/react";
import HUDGlow, { type GlowAlert } from "../HUDGlow";
import { islandPath, type HUDRect } from "../geometry";
import {
  HUD_METRICS,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";

export function Silhouette({
  width,
  height,
  edge,
  flare,
  radius,
  fill,
  outline,
  initialShape,
  duration = 0.4,
}: {
  width: number;
  height: number;
  edge: HUDEdge;
  flare: number;
  radius: number;
  fill: string;
  outline: string;
  initialShape?: {
    width: number;
    height: number;
    flare: number;
    radius: number;
  };
  duration?: number;
}) {
  const path = islandPath(width, height, flare, radius, edge);
  const from = initialShape
    ? islandPath(
        initialShape.width,
        initialShape.height,
        initialShape.flare,
        initialShape.radius,
        edge,
      )
    : path;
  return (
    <svg
      aria-hidden="true"
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        overflow: "visible",
        pointerEvents: "none",
      }}
    >
      <motion.path
        initial={initialShape ? { d: from } : false}
        animate={{ d: path }}
        exit={{ d: from }}
        transition={{ duration, ease: HUD_METRICS.ease }}
        fill={fill}
        stroke={outline}
        strokeWidth={0.7}
      />
    </svg>
  );
}

/** One interpolated geometry drives the notch's canvas, path and glow. */
export function useNotchGeometry(
  frame: HUDRect,
  flare: number,
  radius: number,
  reduced: boolean,
) {
  const x = useMotionValue(frame.x),
    y = useMotionValue(frame.y);
  const width = useMotionValue(frame.width),
    height = useMotionValue(frame.height);
  const shoulder = useMotionValue(flare),
    corner = useMotionValue(radius);
  useLayoutEffect(() => {
    if (reduced) {
      x.set(frame.x);
      y.set(frame.y);
      width.set(frame.width);
      height.set(frame.height);
      shoulder.set(flare);
      corner.set(radius);
      return;
    }
    const origin = {
      x: x.get(),
      y: y.get(),
      width: width.get(),
      height: height.get(),
      flare: shoulder.get(),
      radius: corner.get(),
    };
    const tween = animate(0, 1, {
      duration: HUD_METRICS.duration,
      ease: HUD_METRICS.ease,
      onUpdate: (progress) => {
        x.set(origin.x + (frame.x - origin.x) * progress);
        y.set(origin.y + (frame.y - origin.y) * progress);
        width.set(origin.width + (frame.width - origin.width) * progress);
        height.set(origin.height + (frame.height - origin.height) * progress);
        shoulder.set(origin.flare + (flare - origin.flare) * progress);
        corner.set(origin.radius + (radius - origin.radius) * progress);
      },
    });
    return () => tween.stop();
  }, [
    frame.x,
    frame.y,
    frame.width,
    frame.height,
    flare,
    radius,
    reduced,
    x,
    y,
    width,
    height,
    shoulder,
    corner,
  ]);
  const path = useTransform(() =>
    islandPath(width.get(), height.get(), shoulder.get(), corner.get()),
  );
  const clip = useTransform(() => `path("${path.get()}")`);
  const coreX = useTransform(() => x.get() + shoulder.get());
  return useMemo(
    () => ({ x, y, width, height, shoulder, corner, coreX, path, clip }),
    [x, y, width, height, shoulder, corner, coreX, path, clip],
  );
}

export const NotchGlow = memo(function NotchGlow({
  geometry,
  config,
  vendors,
  colors,
  reduceMotion,
  alert = null,
}: {
  geometry: ReturnType<typeof useNotchGeometry>;
  config: HUDConfig;
  vendors: HUDVendor[];
  /** The rim's colours: the vendors' with readings, none while detection is paused. */
  colors: string[];
  reduceMotion: boolean;
  alert?: GlowAlert | null;
}) {
  const read = () => ({
    width: geometry.width.get() - 2 * geometry.shoulder.get(),
    height: geometry.height.get(),
    radius: geometry.corner.get(),
    flare: geometry.shoulder.get(),
  });
  const [size, setSize] = useState(read);
  const update = () => setSize(read());
  useMotionValueEvent(geometry.width, "change", update);
  useMotionValueEvent(geometry.height, "change", update);
  useMotionValueEvent(geometry.shoulder, "change", update);
  useMotionValueEvent(geometry.corner, "change", update);
  const active = vendors.some((vendor) => vendor.working > 0);
  return (
    <motion.div
      aria-hidden="true"
      style={{
        position: "absolute",
        left: geometry.coreX,
        top: geometry.y,
        pointerEvents: "none",
        zIndex: 1,
      }}
    >
      <HUDGlow
        width={size.width}
        height={size.height}
        radius={size.radius}
        flare={size.flare}
        attached
        outwardOnly={config.glowOutwardOnly}
        style={config.glowStyle}
        effect={config.glowEffect}
        colors={colors}
        active={active}
        period={active ? config.breathSeconds : config.idleBreathSeconds}
        amplitude={config.breathAmplitude}
        brightness={config.glowBrightness}
        range={config.glowRange}
        blur={config.glowBlur}
        pitch={config.glowGridPitch}
        core={config.glowGridCore}
        fade={config.glowGridFade}
        density={config.glowGridDensity}
        reduceMotion={reduceMotion}
        alert={alert}
      />
    </motion.div>
  );
});
