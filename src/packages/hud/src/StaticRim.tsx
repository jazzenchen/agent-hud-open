import { useLayoutEffect, useMemo, useRef } from "react";
import { cssChannels } from "./glow-gpu";
import type { RimTrack } from "./rim-track";
import type { HUDGlowProps } from "./HUDGlow";

const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
// Local 5 x 7 raster forms of .:-=+*#%@01; no remote font request or glyph atlas.
const GLYPH_LO = [
  134217728, 138416256, 1015808, 32537600, 139432064, 720353952, 2515893578,
  438440307, 2212165166, 2738546222, 2286031044,
];
const GLYPH_HI = [1, 0, 0, 0, 0, 0, 2, 3, 3, 3, 3];

type RGB = [number, number, number];
const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

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
export function StaticRim({
  box,
  track,
  padding,
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
> & {
  box: { width: number; height: number };
  track: RimTrack;
  padding: number;
  stopCount: number;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  const rgb = useMemo<RGB[]>(
    () =>
      colors.map((color) => {
        const [r, g, b] = cssChannels(color);
        return [r, g, b];
      }),
    [colors],
  );
  const draw = () => {
    const canvas = ref.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const { x: ox, y: oy, width, height, radius, flare } = track.get();
    const w = Math.ceil(box.width + padding * 2),
      h = Math.ceil(box.height + padding * 2);
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    const image = context.createImageData(w, h);
    const step = style === "braille" ? pitch / 2 : pitch;
    // The silhouette's own canvas inside the box; nothing outside it is inked.
    const sw = Math.ceil(width + padding * 2),
      sh = Math.ceil(height + padding * 2);
    const left = clamp(Math.floor(ox), 0, w),
      right = clamp(Math.ceil(ox + sw), 0, w);
    const top = clamp(Math.floor(oy), 0, h),
      bottom = clamp(Math.ceil(oy + sh), 0, h);
    // The palette varies only along x. Resolve it once per column, rather than
    // repeating the same interpolation for every pixel in the contour's height.
    const gradient = new Uint8ClampedArray(w * 3);
    for (let x = left; x < right; x++) {
      const col = Math.floor((x - ox - padding) / step);
      const sx = style === "blur" ? x - ox : (col + 0.5) * step + padding;
      const location = clamp((sx / sw) * stopCount - 0.5, 0, stopCount - 1);
      const index = Math.min(2, Math.floor(location));
      const fraction = location - index;
      for (let channel = 0; channel < 3; channel++)
        gradient[x * 3 + channel] =
          rgb[index][channel] * (1 - fraction) +
          rgb[index + 1][channel] * fraction;
    }
    for (let y = top; y < bottom; y++)
      for (let x = left; x < right; x++) {
        // X, Y: this pixel on the silhouette's own canvas.
        const X = x - ox,
          Y = y - oy;
        const px = X - padding,
          py = Y - padding;
        const pixelDistance = contour(px, py, width, height, radius, attached);
        if (pixelDistance < 0) continue;
        const lip = Math.min(flare, height / 2);
        if (attached && lip > 0 && py >= 0 && py < lip) {
          const outside = lip * (1 - Math.sqrt(py / lip)) ** 2;
          if (px >= -outside && px <= width + outside) continue;
        }
        const col = Math.floor((X - padding) / step),
          row = Math.floor((Y - padding - height) / step);
        const sx = style === "blur" ? X : (col + 0.5) * step + padding;
        const sy = style === "blur" ? Y : (row + 0.5) * step + padding + height;
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
        const lx = (X - sx) / step,
          ly = (Y - sy) / step;
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
  };
  const latestDraw = useRef(draw);
  latestDraw.current = draw;
  // Before paint, so a new box or look never shows the old picture
  // stretched over it.
  useLayoutEffect(draw, [
    box.width,
    box.height,
    track,
    padding,
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
  useLayoutEffect(() => track.subscribe(() => latestDraw.current()), [track]);
  return (
    <canvas
      ref={ref}
      style={{ width: "100%", height: "100%", display: "block" }}
    />
  );
}
