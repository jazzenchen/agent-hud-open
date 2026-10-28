import { memo, useEffect, useMemo, useRef, useState } from "react";
import { startGpuPaint, type GpuPaint } from "./glow-gpu";
import { flashMask, pulseFrames, type GlowAlert } from "./glow-alert";
import {
  breathFrames,
  breathTiming,
  glowMotion,
  halfBreath,
  retimedBreath,
} from "./glow-motion";

export { glowAlert, type GlowAlert } from "./glow-alert";

/** The original GlowStyle / GlowEffect enums, not unrelated shader presets. */
export type GlowStyle =
  "blur" | "dots" | "ascii" | "blocks" | "braille" | "binary";
export type GlowEffect =
  "breathe" | "flow" | "scan" | "ripple" | "shimmer" | "boot";

export interface HUDGlowProps {
  /** Size of the silhouette this rim wraps, in CSS pixels. */
  width: number;
  height: number;
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
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
// Local 5 x 7 raster forms of .:-=+*#%@01; no remote font request or glyph atlas.
const GLYPH_LO = [
  134217728, 138416256, 1015808, 32537600, 139432064, 720353952, 2515893578,
  438440307, 2212165166, 2738546222, 2286031044,
];
const GLYPH_HI = [1, 0, 0, 0, 0, 0, 2, 3, 3, 3, 3];

/**
 * The rim's WGSL paint body, drawn by glow-gpu.
 * Contour, Gaussian grid falloff and six clocks follow GlowMatrix / GlowMotion.
 * The output is transparent inside the HUD and outside the rim; it never paints a background.
 * `clock` is seconds since the rim appeared. GpuRim advances it only while a
 * pattern moves, so a resting or breathing rim is drawn once, not every frame.
 */
const RIM_NUMBERS = [
  "stripWidth",
  "stripHeight",
  "padding",
  "corner",
  "shoulder",
  "attachedEdge",
  "pattern",
  "motionKind",
  "cycle",
  "depth",
  "strength",
  "reach",
  "outwardFalloff",
  "feather",
  "cellPitch",
  "coreRows",
  "fadeRows",
  "cellDensity",
  "clock",
  "stopCount",
] as const;
const RIM_COLORS = ["colorA", "colorB", "colorC", "colorD"] as const;
const RIM_PAINT = /* wgsl */ `
    let canvas = vec2f(stripWidth, stripHeight) + vec2f(padding * 2.0);
    let pixel = uv * canvas;
    let pixelPoint = pixel - vec2f(padding);
    let pixelRadius = min(corner, min(stripWidth, stripHeight) * 0.5);
    var pixelDistance: f32;
    if (attachedEdge > 0.5) {
      let centerX = clamp(pixelPoint.x, pixelRadius, stripWidth - pixelRadius);
      pixelDistance = length(vec2f(pixelPoint.x - centerX, max(pixelPoint.y - stripHeight + pixelRadius, 0.0))) - pixelRadius;
      if (pixelPoint.y < stripHeight - pixelRadius) { pixelDistance = max(-pixelPoint.x, pixelPoint.x - stripWidth); }
      let lip = min(shoulder, stripHeight * 0.5);
      if (lip > 0.0 && pixelPoint.y >= 0.0 && pixelPoint.y < lip) {
        let outside = lip * pow(1.0 - sqrt(pixelPoint.y / lip), 2.0);
        if (pixelPoint.x >= -outside && pixelPoint.x <= stripWidth + outside) { return vec4f(0.0); }
      }
    } else {
      let q = abs(pixelPoint - vec2f(stripWidth, stripHeight) * 0.5) - vec2f(stripWidth, stripHeight) * 0.5 + vec2f(pixelRadius);
      pixelDistance = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - pixelRadius;
    }
    // Cell centres choose a mark; its actual pixels must remain outside the silhouette.
    if (pixelDistance < 0.0) { return vec4f(0.0); }
    let pitch = max(0.5, cellPitch);
    let rowPitch = select(pitch, pitch * 0.5, pattern > 3.5 && pattern < 4.5);
    let columnPitch = select(pitch, pitch * 0.5, pattern > 3.5 && pattern < 4.5);
    let cell = floor((pixel - vec2f(padding, padding + stripHeight)) / vec2f(columnPitch, rowPitch));
    var samplePoint = pixel;
    if (pattern > 0.5) {
      samplePoint = (cell + vec2f(0.5)) * vec2f(columnPitch, rowPitch) + vec2f(padding, padding + stripHeight);
    }
    let p = samplePoint - vec2f(padding);
    let r = min(corner, min(stripWidth, stripHeight) * 0.5);
    var distanceFromRim: f32;
    if (attachedEdge > 0.5) {
      let x = clamp(p.x, r, stripWidth - r);
      distanceFromRim = length(vec2f(p.x - x, max(p.y - stripHeight + r, 0.0))) - r;
      if (p.y < stripHeight - r) { distanceFromRim = max(-p.x, p.x - stripWidth); }
    } else {
      let q = abs(p - vec2f(stripWidth, stripHeight) * 0.5) - vec2f(stripWidth, stripHeight) * 0.5 + vec2f(r);
      distanceFromRim = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - r;
    }
    if (distanceFromRim < 0.0) { return vec4f(0.0); }
    var value: f32;
    if (pattern < 0.5) {
      let tail = max(0.5, reach + feather * 3.0);
      let power = max(1.0, 8.0 * feather / max(0.5, reach));
      value = pow(max(0.0, 1.0 - distanceFromRim / tail), power);
      if (outwardFalloff < 0.5) {
        let featherWidth = min(reach, feather * 2.0);
        let t = clamp((distanceFromRim - reach + featherWidth) / max(0.5, featherWidth + feather * 3.0), 0.0, 1.0);
        value = (1.0 - t) * (1.0 - t);
      }
      if (feather <= 0.0) { value = select(0.0, 1.0, distanceFromRim <= reach); }
    } else {
      let fall = max(0.0, distanceFromRim / pitch - coreRows) / max(0.01, fadeRows);
      value = exp(-2.995732 * fall * fall);
      if (fall > 1.0) { return vec4f(0.0); }
    }
    let phase = clock / max(0.25, cycle);
    var gain = 1.0;
    var location = clamp(samplePoint.x / canvas.x, 0.0, 1.0);
    let noise = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + floor(phase * 24.0) * 37.719) * 43758.5453);
    if (motionKind < 0.5) {
      gain = 1.0 - depth * (0.5 - 0.5 * cos(6.2831853 * phase));
    } else if (motionKind < 1.5) {
      location = 1.0 - abs(2.0 * fract(location * 0.5 + phase) - 1.0);
    } else if (motionKind < 2.5) {
      let center = fract(phase) * (canvas.x + 160.0 / 14.0 * pitch) - 80.0 / 14.0 * pitch;
      let offset = (samplePoint.x - center) / (40.0 / 14.0 * pitch);
      gain = 0.5 + 0.7 * exp(-offset * offset);
    } else if (motionKind < 3.5) {
      gain = 0.55 + 0.45 * (0.5 + 0.5 * sin(6.2831853 * (distanceFromRim / (3.0 * pitch) - phase)));
    } else if (motionKind < 4.5) {
      let flash = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (floor(phase * 24.0) + 53.0) * 37.719) * 43758.5453);
      gain = select(0.5 + 0.3 * noise, 1.2, flash < 0.12);
    } else {
      let boot = fract(phase) * 4.4;
      if (boot < 2.2) { gain = select(0.0, 1.0, distanceFromRim < boot / 2.2 * 8.0 * pitch); }
      else if (boot >= 3.6) { gain = 1.0 - (boot - 3.6) / 0.8; }
    }
    value = clamp(value * gain, 0.0, 1.0);
    if (value < 0.06) { return vec4f(0.0); }
    let gradientLocation = clamp(location * stopCount - 0.5, 0.0, stopCount - 1.0);
    var rgb = mix(colorA.rgb, colorB.rgb, clamp(gradientLocation, 0.0, 1.0));
    rgb = mix(rgb, colorC.rgb, clamp(gradientLocation - 1.0, 0.0, 1.0));
    rgb = mix(rgb, colorD.rgb, clamp(gradientLocation - 2.0, 0.0, 1.0));
    if (motionKind > 0.5 && motionKind < 1.5) {
      let offset = (samplePoint.x / canvas.x - (fract(phase) * 1.5 - 0.25)) / 0.25;
      let sheen = pow(cos(clamp(offset, -1.0, 1.0) * 1.5707963), 2.0);
      rgb = mix(rgb, vec3f(1.0), sheen * 0.3);
    }
    var alpha = value;
    if (pattern > 0.5) {
      let local = (pixel - samplePoint) / vec2f(columnPitch, rowPitch);
      let bayer = array<f32, 16>(0.0,8.0,2.0,10.0,12.0,4.0,14.0,6.0,3.0,11.0,1.0,9.0,15.0,7.0,13.0,5.0);
      let bx = u32(i32(cell.x) & 3); let by = u32(i32(cell.y) & 3);
      let threshold = (bayer[by * 4u + bx] + 0.5) / 16.0;
      var jitter = 0.0;
      let noiseStep = floor(phase * select(24.0, 35.2, motionKind > 4.5));
      if (motionKind > 3.5 && motionKind < 4.5) {
        let twinkle = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (noiseStep + 101.0) * 37.719) * 43758.5453);
        if (twinkle < 0.1) { jitter = 2.0; }
      } else if (motionKind > 4.5) {
        let boot = fract(phase) * 4.4;
        if (boot < 2.2 && abs(distanceFromRim - boot / 2.2 * 8.0 * pitch) < 0.8 * pitch) { jitter = 9.0; }
      }
      let shiftNoise = fract(sin(cell.x * 12.9898 + cell.y * 78.233 + (noiseStep + 7.0) * 37.719) * 43758.5453);
      let shift = min(jitter * 2.0, floor(shiftNoise * (jitter * 2.0 + 1.0))) - jitter;
      if (pattern < 1.5) {
        let dotRadius = cellDensity * (0.07 + 0.45 * sqrt(value));
        alpha = 1.0 - smoothstep(dotRadius - 0.045, dotRadius, length(local));
      } else if (pattern > 3.5 && pattern < 4.5) {
        alpha = select(0.0, 0.45 + 0.55 * sqrt(value), 0.1 + 0.9 * value > threshold);
        alpha *= 1.0 - smoothstep(cellDensity * 0.30, cellDensity * 0.34, length(local));
      } else if (pattern > 2.5 && pattern < 3.5) {
        let level = clamp(clamp(floor(value * 4.0 + threshold - 0.5), 0.0, 3.0) + shift, 0.0, 3.0);
        let micro = floor((local + vec2f(0.5)) * vec2f(4.0));
        let mark = fract((micro.x + micro.y * 3.0) / 4.0);
        alpha = select(0.0, 0.45 + 0.55 * sqrt(value), mark < (level + 1.0) * 0.25);
        if (max(abs(local.x), abs(local.y)) > cellDensity * 0.5) { alpha = 0.0; }
      } else {
        var letter = u32(clamp(clamp(floor(value * 9.0 + threshold - 0.5), 0.0, 8.0) + shift, 0.0, 8.0));
        if (pattern > 4.5) { letter = select(9u, 10u, 0.1 + 0.9 * value > threshold); }
        let glyphPoint = (local / max(0.1, cellDensity) + vec2f(0.5)) * vec2f(5.0, 7.0);
        if (any(glyphPoint < vec2f(0.0)) || any(glyphPoint >= vec2f(5.0,7.0))) { return vec4f(0.0); }
        let bit = u32(floor(glyphPoint.y)) * 5u + u32(floor(glyphPoint.x));
        let lo = array<u32, 11>(134217728u,138416256u,1015808u,32537600u,139432064u,720353952u,2515893578u,438440307u,2212165166u,2738546222u,2286031044u);
        let hi = array<u32, 11>(1u,0u,0u,0u,0u,0u,2u,3u,3u,3u,3u);
        let bits = select(lo[letter], hi[letter], bit >= 32u);
        alpha = f32((bits >> (bit % 32u)) & 1u) * (0.45 + 0.55 * sqrt(value));
        if (letter == 9u) { alpha *= 0.35; }
      }
    }
    return vec4f(rgb, alpha * strength);
`;

type RGB = [number, number, number];
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

function contour(
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
  attached: boolean,
) {
  const r = Math.min(radius, width / 2, height / 2);
  if (attached)
    return y < height - r
      ? Math.max(-x, x - width)
      : Math.hypot(x - clamp(x, r, width - r), Math.max(y - height + r, 0)) - r;
  const qx = Math.abs(x - width / 2) - width / 2 + r;
  const qy = Math.abs(y - height / 2) - height / 2 + r;
  return (
    Math.hypot(Math.max(0, qx), Math.max(0, qy)) +
    Math.min(Math.max(qx, qy), 0) -
    r
  );
}

/** CPU rendering is a still of the same contour, falloff, grid and local glyph masks. */
function StaticRim({
  width,
  height,
  padding,
  radius,
  flare,
  attached,
  style,
  colors,
  range,
  outwardOnly,
  blur,
  pitch,
  core,
  fade,
  density,
  brightness,
  stopCount,
}: Required<
  Pick<
    HUDGlowProps,
    | "width"
    | "height"
    | "radius"
    | "flare"
    | "attached"
    | "style"
    | "colors"
    | "range"
    | "outwardOnly"
    | "blur"
    | "pitch"
    | "core"
    | "fade"
    | "density"
    | "brightness"
  >
> & { padding: number; stopCount: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const rgb = useMemo<RGB[]>(() => {
    const probe = document.createElement("canvas");
    probe.width = colors.length;
    probe.height = 1;
    const context = probe.getContext("2d", { willReadFrequently: true });
    if (!context) return colors.map(() => [110, 110, 115]);
    colors.forEach((color, index) => {
      context.fillStyle = color;
      context.fillRect(index, 0, 1, 1);
    });
    const pixels = context.getImageData(0, 0, colors.length, 1).data;
    return colors.map((_, index) => [
      pixels[index * 4],
      pixels[index * 4 + 1],
      pixels[index * 4 + 2],
    ]);
  }, [colors]);
  useEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const w = Math.ceil(width + padding * 2),
      h = Math.ceil(height + padding * 2);
    canvas.width = w;
    canvas.height = h;
    const image = context.createImageData(w, h);
    const step = style === "braille" ? pitch / 2 : pitch;
    // The palette varies only along x. Resolve it once per column, rather than
    // repeating the same interpolation for every pixel in the contour's height.
    const gradient = new Uint8ClampedArray(w * 3);
    for (let x = 0; x < w; x++) {
      const col = Math.floor((x - padding) / step);
      const sx = style === "blur" ? x : (col + 0.5) * step + padding;
      const location = clamp((sx / w) * stopCount - 0.5, 0, stopCount - 1);
      const index = Math.min(2, Math.floor(location));
      const fraction = location - index;
      for (let channel = 0; channel < 3; channel++)
        gradient[x * 3 + channel] =
          rgb[index][channel] * (1 - fraction) +
          rgb[index + 1][channel] * fraction;
    }
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const px = x - padding,
          py = y - padding;
        const pixelDistance = contour(px, py, width, height, radius, attached);
        if (pixelDistance < 0) continue;
        const lip = Math.min(flare, height / 2);
        if (attached && lip > 0 && py >= 0 && py < lip) {
          const outside = lip * (1 - Math.sqrt(py / lip)) ** 2;
          if (px >= -outside && px <= width + outside) continue;
        }
        const col = Math.floor((x - padding) / step),
          row = Math.floor((y - padding - height) / step);
        const sx = style === "blur" ? x : (col + 0.5) * step + padding;
        const sy = style === "blur" ? y : (row + 0.5) * step + padding + height;
        const distance =
          style === "blur"
            ? pixelDistance
            : contour(
                sx - padding,
                sy - padding,
                width,
                height,
                radius,
                attached,
              );
        if (distance < 0) continue;
        const fall =
          Math.max(0, distance / pitch - core) / Math.max(0.01, fade);
        let value =
          style === "blur"
            ? Math.pow(
                Math.max(0, 1 - distance / Math.max(0.5, range + blur * 3)),
                Math.max(1, (8 * blur) / Math.max(0.5, range)),
              )
            : Math.exp(-2.995732 * fall * fall);
        if (style === "blur" && !outwardOnly) {
          const featherWidth = Math.min(range, blur * 2);
          const t = clamp(
            (distance - range + featherWidth) /
              Math.max(0.5, featherWidth + blur * 3),
            0,
            1,
          );
          value = (1 - t) ** 2;
        }
        if (style === "blur" && blur <= 0) value = distance <= range ? 1 : 0;
        if ((style !== "blur" && fall > 1) || value < 0.06) continue;
        const lx = (x - sx) / step,
          ly = (y - sy) / step;
        const threshold = (BAYER[(row & 3) * 4 + (col & 3)] + 0.5) / 16;
        if (style === "dots")
          value = clamp(
            (density * (0.07 + 0.45 * Math.sqrt(value)) - Math.hypot(lx, ly)) /
              0.045,
            0,
            1,
          );
        else if (style === "braille")
          value =
            0.1 + 0.9 * value > threshold && Math.hypot(lx, ly) < density * 0.34
              ? 0.45 + 0.55 * Math.sqrt(value)
              : 0;
        else if (style === "blocks") {
          const level = Math.min(3, Math.floor(value * 4 + threshold - 0.5));
          const micro =
            ((Math.floor((lx + 0.5) * 4) + Math.floor((ly + 0.5) * 4) * 3) %
              4) /
            4;
          value =
            micro < (level + 1) / 4 &&
            Math.max(Math.abs(lx), Math.abs(ly)) <= density / 2
              ? 0.45 + 0.55 * Math.sqrt(value)
              : 0;
        } else if (style === "ascii" || style === "binary") {
          const letter =
            style === "binary"
              ? 0.1 + 0.9 * value > threshold
                ? 10
                : 9
              : clamp(Math.floor(value * 9 + threshold - 0.5), 0, 8);
          const gx = Math.floor((lx / density + 0.5) * 5),
            gy = Math.floor((ly / density + 0.5) * 7),
            bit = gy * 5 + gx;
          const ink =
            gx >= 0 &&
            gx < 5 &&
            gy >= 0 &&
            gy < 7 &&
            ((bit < 32 ? GLYPH_LO[letter] : GLYPH_HI[letter]) >>> (bit % 32)) &
              1;
          value = ink
            ? (0.45 + 0.55 * Math.sqrt(value)) * (letter === 9 ? 0.35 : 1)
            : 0;
        }
        const offset = (y * w + x) * 4;
        for (let channel = 0; channel < 3; channel++)
          image.data[offset + channel] = gradient[x * 3 + channel];
        image.data[offset + 3] = Math.round(value * brightness * 255);
      }
    context.putImageData(image, 0, 0);
  }, [
    width,
    height,
    padding,
    radius,
    flare,
    attached,
    style,
    rgb,
    range,
    outwardOnly,
    blur,
    pitch,
    core,
    fade,
    density,
    brightness,
    stopCount,
  ]);
  return (
    <canvas
      ref={ref}
      style={{ width: "100%", height: "100%", display: "block" }}
    />
  );
}

type RimValues = Record<string, number | string>;

/**
 * The rim on its own WebGPU canvas, drawing only when asked: a change of size
 * or value draws one frame, and a moving pattern advances `clock` at
 * `framesPerSecond`.
 */
function GpuRim({
  values,
  framesPerSecond,
  shown,
  onReady,
  onUnavailable,
}: {
  values: RimValues;
  framesPerSecond: number;
  shown: boolean;
  onReady: () => void;
  onUnavailable: () => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const live = useRef<GpuPaint | null>(null);
  const latest = useRef(values);
  latest.current = values;
  const callbacks = useRef({ onReady, onUnavailable });
  callbacks.current = { onReady, onUnavailable };
  const start = useRef(0);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    start.current = performance.now();
    const paint = startGpuPaint({
      canvas: element,
      numbers: RIM_NUMBERS,
      colors: RIM_COLORS,
      paint: RIM_PAINT,
      values: { ...latest.current, clock: 0 },
      onReady: () => callbacks.current.onReady(),
      onUnavailable: () => callbacks.current.onUnavailable(),
    });
    live.current = paint;
    return () => {
      live.current = null;
      paint.dispose();
    };
  }, []);
  useEffect(() => {
    live.current?.update(values);
  });
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
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        opacity: shown ? 1 : 0,
      }}
    >
      <canvas
        ref={canvas}
        data-renderer="webgpu"
        style={{ width: "100%", height: "100%", display: "block" }}
      />
    </div>
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
  className,
}: HUDGlowProps) {
  const [systemReduced, setSystemReduced] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
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
  // The soft breath is an opacity animation of the drawn rim, which WebKit
  // hands to Core Animation, as Open's applyBreathing does: no page or GPU
  // work per frame. A change of period keeps the glow where it is in its
  // breath; a change of depth only reshapes the curve.
  const drawn = width > 0 && height > 0 && strength > 0;
  const breath = useRef<HTMLDivElement | null>(null);
  const breathing = useRef<{ animation: Animation; amplitude: number } | null>(
    null,
  );
  useEffect(() => {
    const element = breath.current;
    if (!motion.pulse || !element || typeof element.animate !== "function")
      return;
    const animation = element.animate(
      breathFrames(amplitude),
      breathTiming(cycle),
    );
    breathing.current = { animation, amplitude };
    return () => {
      breathing.current = null;
      animation.cancel();
    };
  }, [motion.pulse, drawn]);
  useEffect(() => {
    const running = breathing.current;
    if (!running || running.amplitude === amplitude) return;
    running.amplitude = amplitude;
    const keyframes = running.animation.effect;
    if (keyframes instanceof KeyframeEffect)
      keyframes.setKeyframes(breathFrames(amplitude));
  }, [amplitude]);
  useEffect(() => {
    const animation = breathing.current?.animation;
    const timing = animation?.effect;
    if (!animation || !timing) return;
    const from = Number(timing.getTiming().duration);
    const to = halfBreath(cycle);
    if (from === to) return;
    const time = Number(animation.currentTime ?? 0);
    timing.updateTiming({ duration: to });
    animation.currentTime = retimedBreath(time, from, to);
  }, [cycle]);
  // Each alert flashes once, when it arrives.
  const flash = useRef<HTMLDivElement | null>(null);
  const [flashing, setFlashing] = useState<GlowAlert | null>(null);
  useEffect(() => {
    // A new alert flashes; one that is gone stops its flash at once.
    setFlashing(
      alert && alert.id !== flashing?.id ? alert : alert ? flashing : null,
    );
  }, [alert?.id]);
  useEffect(() => {
    const element = flash.current;
    if (!flashing || !element || typeof element.animate !== "function") return;
    const animation = element.animate(
      pulseFrames(flashing.warning, reduceMotion || systemReduced),
      {
        duration: 1800,
        easing: "linear",
        fill: "forwards",
      },
    );
    animation.onfinish = () =>
      setFlashing((current) => (current?.id === flashing.id ? null : current));
    return () => animation.cancel();
  }, [flashing?.id]);
  if (!drawn) return null;
  const mask = flashing ? flashMask(flashing.span, width, padding) : undefined;
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
            width={width}
            height={height}
            padding={padding}
            radius={radius}
            flare={flare}
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
            framesPerSecond={motion.framesPerSecond}
            shown={gpuReady}
            onReady={() => setGpuReady(true)}
            onUnavailable={() => {
              setGpuFailed(true);
              setGpuReady(false);
            }}
            values={{
              stripWidth: width,
              stripHeight: height,
              padding,
              corner: radius,
              shoulder: flare,
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
          ref={flash}
          data-glow-alert={flashing.id}
          style={{
            position: "absolute",
            inset: 0,
            opacity: 0,
            maskImage: mask,
            WebkitMaskImage: mask,
          }}
        >
          <StaticRim
            width={width}
            height={height}
            padding={padding}
            radius={radius}
            flare={flare}
            attached={attached}
            style={style}
            colors={palette([flashing.color])}
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
