import { memo, useEffect, useMemo, useState } from "react";
import type { GlowAlert } from "./glow-alert";
import { fixedTrack, type RimTrack } from "./rim-track";
import { glowMotion } from "./glow-motion";
import { useGlowBreath } from "./glow-breath";
import { useGlowFlash } from "./glow-flash";
import { GpuRim } from "./GpuRim";
import { StaticRim } from "./StaticRim";

export { glowAlert, type GlowAlert } from "./glow-alert";

/** The original GlowStyle / GlowEffect enums, not unrelated shader presets. */
export type GlowStyle =
  "blur" | "dots" | "ascii" | "blocks" | "braille" | "binary";
export type GlowEffect =
  "breathe" | "flow" | "scan" | "ripple" | "shimmer" | "boot";

export interface HUDGlowProps {
  /** Size of the silhouette this rim wraps, in CSS pixels; with `track`, the
   * box the moving silhouette stays inside. */
  width: number;
  height: number;
  /** A silhouette that moves inside the box: the rim follows it in the frame
   * it moves, without a render, and `radius` and `flare` come from it. */
  track?: RimTrack;
  style?: GlowStyle;
  effect?: GlowEffect;
  colors?: string[];
  active?: boolean;
  period?: number;
  amplitude?: number;
  brightness?: number;
  range?: number;
  outwardOnly?: boolean;
  blur?: number;
  pitch?: number;
  core?: number;
  fade?: number;
  density?: number;
  radius?: number;
  /** Concave contact shoulders outside the core; excluded from the rim's ink. */
  flare?: number;
  /** Notch joins the screen's top edge; a floating logo strip has four rounded corners. */
  attached?: boolean;
  reduceMotion?: boolean;
  /** The island alert that just arrived: the rim flashes in its colour over
   * its sender's part of the gradient (`glowAlert`). */
  alert?: GlowAlert | null;
  className?: string;
}

const STYLES: GlowStyle[] = [
  "blur",
  "dots",
  "ascii",
  "blocks",
  "braille",
  "binary",
];
const EFFECTS: GlowEffect[] = [
  "breathe",
  "flow",
  "scan",
  "ripple",
  "shimmer",
  "boot",
];

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

function palette(colors: string[]): string[] {
  if (!colors.length) return ["#9a9aa0", "#9a9aa0", "#9a9aa0", "#9a9aa0"];
  if (colors.length <= 4)
    return Array.from(
      { length: 4 },
      (_, index) => colors[Math.min(index, colors.length - 1)],
    );
  return Array.from(
    { length: 4 },
    (_, index) => colors[Math.round((index / 3) * (colors.length - 1))],
  );
}

export const HUDGlow = memo(function HUDGlow({
  width,
  height,
  style = "blur",
  effect = "breathe",
  colors = [],
  active = true,
  period,
  amplitude = 0.6,
  brightness = 0.9,
  range = 14,
  outwardOnly = true,
  blur = 8,
  pitch = 10,
  core = 0,
  fade = 5,
  density = 1,
  radius = 12,
  flare = 0,
  attached = true,
  reduceMotion = false,
  alert = null,
  track,
  className,
}: HUDGlowProps) {
  const [systemReduced, setSystemReduced] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [gpuReady, setGpuReady] = useState(false);
  const [gpuFailed, setGpuFailed] = useState(false);
  const [visible, setVisible] = useState(() => !document.hidden);
  useEffect(() => {
    const change = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", change);
    return () => document.removeEventListener("visibilitychange", change);
  }, []);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const change = () => setSystemReduced(media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  const colorKey = JSON.stringify(colors);
  const stops = useMemo(() => palette(JSON.parse(colorKey)), [colorKey]);
  const stopCount = clamp(colors.length, 1, 4);
  // GlowAppearance.resolve: without quota readings, or while detection is
  // paused (no colours are passed then), the rim is StatusPalette.idle grey
  // at 0.35 and still. Do not start the GPU at all.
  const noReadings = colors.length === 0;
  const strength = noReadings ? 0.35 : brightness;
  const padding =
    style === "blur" ? range + blur * 3 : (core + fade) * pitch + pitch;
  const moves = !(noReadings || reduceMotion || systemReduced || !visible);
  const still = !moves || gpuFailed;
  const cycle = period ?? (active ? 3 : 7);
  const motion = glowMotion(style, effect, cycle, moves, !gpuFailed);
  useEffect(() => {
    if (still) setGpuReady(false);
  }, [still]);
  const drawn = width > 0 && height > 0 && strength > 0;
  const breath = useGlowBreath(motion.pulse, drawn, amplitude, cycle);
  // Without a track the silhouette fills the box, as it always did.
  const box = useMemo(() => ({ width, height }), [width, height]);
  const fixed = useMemo(
    () => fixedTrack({ x: 0, y: 0, width, height, radius, flare }),
    [width, height, radius, flare],
  );
  const shape = track ?? fixed;
  const { flash, flashing, maskAt, flashAnimation, flashEnded } = useGlowFlash({
    alert,
    reduced: reduceMotion || systemReduced,
    shape,
    padding,
    width,
  });
  const flashStops = useMemo(
    () => (flashing ? palette([flashing.color]) : []),
    [flashing?.color],
  );
  if (!drawn) return null;
  const mask = maskAt(shape.get());
  return (
    <div
      aria-hidden="true"
      className={className}
      data-hud-glow={style}
      data-renderer={still || !gpuReady ? "static" : "webgpu"}
      data-motion={
        motion.pulse
          ? "pulse"
          : motion.framesPerSecond > 0
            ? `${motion.framesPerSecond}fps`
            : "still"
      }
      data-fallback-reason={
        noReadings
          ? "no-readings"
          : gpuFailed
            ? "gpu-unavailable"
            : reduceMotion || systemReduced
              ? "reduced-motion"
              : !visible
                ? "hidden"
                : !gpuReady
                  ? "initializing"
                  : undefined
      }
      style={{
        position: "absolute",
        pointerEvents: "none",
        left: -padding,
        top: -padding,
        width: width + padding * 2,
        height: height + padding * 2,
      }}
    >
      <div
        ref={breath}
        data-glow-breath
        style={{ position: "absolute", inset: 0 }}
      >
        {(still || !gpuReady) && (
          <StaticRim
            box={box}
            track={shape}
            padding={padding}
            attached={attached}
            style={style}
            colors={stops}
            range={range}
            outwardOnly={outwardOnly}
            blur={blur}
            pitch={pitch}
            core={core}
            fade={fade}
            density={density}
            brightness={strength}
            stopCount={stopCount}
          />
        )}
        {!still && (
          <GpuRim
            track={shape}
            framesPerSecond={motion.framesPerSecond}
            shown={gpuReady}
            onReady={() => setGpuReady(true)}
            onUnavailable={() => {
              setGpuFailed(true);
              setGpuReady(false);
            }}
            values={{
              boxWidth: width,
              boxHeight: height,
              padding,
              attachedEdge: attached ? 1 : 0,
              pattern: STYLES.indexOf(style),
              motionKind: EFFECTS.indexOf(effect),
              cycle,
              // A pulsing rim is drawn at its peak; the breath dims the layer.
              depth: motion.pulse ? 0 : amplitude,
              strength,
              reach: range,
              outwardFalloff: outwardOnly ? 1 : 0,
              feather: blur,
              cellPitch: pitch,
              coreRows: core,
              fadeRows: fade,
              cellDensity: density,
              colorA: stops[0],
              colorB: stops[1],
              colorC: stops[2],
              colorD: stops[3],
              stopCount,
            }}
          />
        )}
      </div>
      {flashing && (
        <div
          key={flashing.id}
          ref={flash}
          data-glow-alert={flashing.id}
          onAnimationEnd={flashEnded}
          style={{
            position: "absolute",
            inset: 0,
            opacity: 0,
            animation: flashAnimation,
            maskImage: mask,
            WebkitMaskImage: mask,
          }}
        >
          <StaticRim
            box={box}
            track={shape}
            padding={padding}
            attached={attached}
            style={style}
            colors={flashStops}
            range={range}
            outwardOnly={outwardOnly}
            blur={blur}
            pitch={pitch}
            core={core}
            fade={fade}
            density={density}
            brightness={1}
            stopCount={1}
          />
        </div>
      )}
    </div>
  );
});

export default HUDGlow;
