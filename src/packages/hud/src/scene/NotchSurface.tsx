import { memo, useLayoutEffect, useMemo, useState } from "react";
import {
  animate,
  cancelFrame,
  frame as nextFrame,
  motion,
  useMotionValue,
  useMotionValueEvent,
  useTransform,
} from "motion/react";
import HUDGlow, { type GlowAlert } from "../HUDGlow";
import { islandPath, type HUDRect } from "../geometry";
import { rimBox, rimCore, type RimRect, type RimTrack } from "../rim-track";
import {
  HUD_METRICS,
  statusColor,
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
  // The box the rim is drawn in: the core at both ends of the current move.
  const rim = useMotionValue<RimRect>(rimCore(frame, flare));
  useLayoutEffect(() => {
    const target = rimCore(frame, flare);
    if (reduced) {
      x.set(frame.x);
      y.set(frame.y);
      width.set(frame.width);
      height.set(frame.height);
      shoulder.set(flare);
      corner.set(radius);
      rim.set(target);
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
    rim.set(rimBox(rimCore(origin, origin.flare), target));
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
      onComplete: () => rim.set(target),
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
    rim,
  ]);
  const path = useTransform(() =>
    islandPath(width.get(), height.get(), shoulder.get(), corner.get()),
  );
  const clip = useTransform(() => `path("${path.get()}")`);
  const coreX = useTransform(() => x.get() + shoulder.get());
  return useMemo(
    () => ({ x, y, width, height, shoulder, corner, coreX, path, clip, rim }),
    [x, y, width, height, shoulder, corner, coreX, path, clip, rim],
  );
}

export const NotchGlow = memo(function NotchGlow({
  geometry,
  config,
  vendors,
  reduceMotion,
  alert = null,
}: {
  geometry: ReturnType<typeof useNotchGeometry>;
  config: HUDConfig;
  vendors: HUDVendor[];
  reduceMotion: boolean;
  alert?: GlowAlert | null;
}) {
  // The box renders only when a move starts or ends. Every frame between,
  // the rim reads the notch's own values and draws in the frame they change,
  // as the path does; through a render it trailed the notch by a frame or
  // more and left a gap while the panel closed.
  const [box, setBox] = useState(() => geometry.rim.get());
  useMotionValueEvent(geometry.rim, "change", setBox);
  const track = useMemo<RimTrack>(() => {
    const values = [
      geometry.x,
      geometry.y,
      geometry.width,
      geometry.height,
      geometry.shoulder,
      geometry.corner,
    ];
    return {
      get: () => {
        const flare = geometry.shoulder.get();
        return {
          x: geometry.x.get() + flare - box.x,
          y: geometry.y.get() - box.y,
          width: geometry.width.get() - 2 * flare,
          height: geometry.height.get(),
          radius: geometry.corner.get(),
          flare,
        };
      },
      subscribe: (change) => {
        // Once a frame, in the step where motion draws the path.
        const schedule = () => nextFrame.render(change);
        const stops = values.map((value) => value.on("change", schedule));
        return () => {
          stops.forEach((stop) => stop());
          cancelFrame(change);
        };
      },
    };
  }, [geometry, box.x, box.y]);
  const active = vendors.some((vendor) => vendor.working > 0);
  return (
    <div
      aria-hidden="true"
      style={{
        position: "absolute",
        left: box.x,
        top: box.y,
        pointerEvents: "none",
        zIndex: 1,
      }}
    >
      <HUDGlow
        width={box.width}
        height={box.height}
        track={track}
        attached
        outwardOnly={config.glowOutwardOnly}
        style={config.glowStyle}
        effect={config.glowEffect}
        colors={vendors
          .filter((vendor) => vendor.used != null)
          .map((vendor) => statusColor(vendor.level))}
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
    </div>
  );
});
