import type { QuotaDisplay } from "@agent-hud/ui/format";
import {
  HUD_METRICS,
  meterLabel,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "./model";
import {
  isRingStyle,
  simpleLogoSize,
  simpleRingSpec,
  usageArcs,
} from "./usage-meter/arcs";
import { markWindows } from "./usage-meter/mark-windows";

/** Whether a HUD on `edge` runs across the screen. */
export const horizontal = (edge: HUDEdge) =>
  edge === "top" || edge === "bottom";

export interface HUDRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** ScreenHUD.dragSurfaceFrame / dragOutline and HUDDragView.updateOutline. */
export function dragSurfaceGeometry(
  strip: HUDRect,
  config: HUDConfig,
  radius: number,
  screen: { width: number; height: number },
) {
  const grow = (rect: HUDRect, x: number, y: number): HUDRect => ({
    x: rect.x - x,
    y: rect.y - y,
    width: rect.width + x * 2,
    height: rect.height + y * 2,
  });
  let outline: HUDRect | undefined;
  let frame: HUDRect;
  let corner = 7;
  if (config.mode === "simple") {
    const panel = { ...strip };
    if (config.edge === "top") {
      panel.y += 3;
      panel.height -= 3;
    } else if (config.edge === "bottom") panel.height -= 3;
    else if (config.edge === "left") {
      panel.x += 3;
      panel.width -= 3;
    } else panel.width -= 3;
    outline = grow(panel, 2, 2);
    frame = grow(outline, 2, 2);
    corner = radius + 2;
  } else {
    const horizontal =
      config.mode === "notch" ||
      config.edge === "top" ||
      config.edge === "bottom";
    frame = grow(strip, horizontal ? 8 : 0, horizontal ? 0 : 8);
    if (config.mode === "notch") frame.height += 14;
  }
  const right = Math.min(screen.width, frame.x + frame.width),
    bottom = Math.min(screen.height, frame.y + frame.height);
  frame = {
    x: Math.max(0, frame.x),
    y: Math.max(0, frame.y),
    width: Math.max(0, right - Math.max(0, frame.x)),
    height: Math.max(0, bottom - Math.max(0, frame.y)),
  };
  outline ??= grow(frame, -2, -2);
  return {
    frame,
    outline,
    radius: Math.max(
      0,
      Math.min(corner, outline.width / 2, outline.height / 2),
    ),
  };
}
export interface StripMetrics {
  width: number;
  height: number;
  gap: number;
  logo: number;
  mark: number;
  font: number;
  labelGap: number;
  /** The widest of the Simple marks' number labels; 0 when they draw none. */
  label: number;
  /** A Simple mark's bars beside its logo, how long; 0 when it draws none. */
  bar: number;
  padding: { top: number; right: number; bottom: number; left: number };
  radius: number;
  centers: number[];
}
const clamp = (n: number, lo: number, hi: number) =>
  Math.max(lo, Math.min(hi, n));

export function labelWidth(text: string, size: number): number {
  const context = document.createElement("canvas").getContext("2d")!;
  context.font = `${size}px -apple-system, BlinkMacSystemFont, sans-serif`;
  const tabularDigit = Math.max(
    ..."0123456789".split("").map((digit) => context.measureText(digit).width),
  );
  return Math.ceil(
    [...text].reduce(
      (sum, char) =>
        sum +
        (/^[0-9]$/.test(char) ? tabularDigit : context.measureText(char).width),
      0,
    ),
  );
}

export function stripMetrics(
  config: HUDConfig,
  vendors: HUDVendor[],
  display?: QuotaDisplay,
): StripMetrics {
  const horizontal = config.edge === "top" || config.edge === "bottom";
  const n = vendors.length;
  if (config.mode === "notch")
    return {
      width: config.notchWidth,
      height: config.notchHeight,
      gap: 0,
      logo: 0,
      mark: 0,
      font: 0,
      labelGap: 0,
      label: 0,
      bar: 0,
      padding: { top: 0, right: 0, bottom: 0, left: 0 },
      radius: config.hasNotch ? 12 : 10,
      centers: [],
    };
  if (config.mode === "logos") {
    const logo = clamp(config.logoSize, 20, 60),
      gap = logo * clamp(config.gapScale, 0.1, 0.6);
    const thick = logo + clamp(config.logoEdgeDistance, 0, 20) * 2;
    const run = n ? n * logo + (n - 1) * gap : Math.max(32, logo);
    return {
      width: horizontal ? run : thick,
      height: horizontal ? thick : run,
      gap,
      logo,
      mark: logo,
      font: 0,
      labelGap: 0,
      label: 0,
      bar: 0,
      padding: { top: 0, right: 0, bottom: 0, left: 0 },
      radius: 10,
      centers: vendors.map((_, i) => i * (logo + gap) + logo / 2),
    };
  }
  const size = clamp(config.simpleSize, 20, 60),
    style = config.simpleStyle,
    ringed = isRingStyle(style),
    numbers = style === "numbers" || config.showsNumbers;
  // Rings take every mark's logo down to what the agent with the most rings leaves.
  const logo = Math.round(
    style === "rings"
      ? simpleLogoSize(
          size,
          usageArcs(
            "rings",
            Math.max(
              1,
              ...vendors.map((v) => markWindows(v, config.panelLayout).length),
            ),
            simpleRingSpec(size, config.ringWidth),
          ),
        )
      : size * (ringed ? 0.55 : 0.68),
  );
  const mark = ringed ? size : logo;
  const bar = style === "bars" ? Math.round(size * 1.09) : 0;
  const font = Math.round(HUD_METRICS.dockTextSize * Math.max(1, size / 26));
  const labelGap = Math.round(size * 0.23);
  const gap = Math.round(
    size *
      clamp(config.simpleGapScale, 0.1, 0.6) *
      (numbers || !horizontal ? 1 : 0.7),
  );
  const tight = Math.max(3, Math.round(size * 0.18)),
    loose = Math.round(size * 0.45);
  const left = ringed ? tight + 1 : loose,
    right = numbers ? loose : left;
  const padding = {
    top: tight + (horizontal ? 0 : 3),
    right,
    bottom: tight + (horizontal ? 0 : 3),
    left,
  };
  const labels = vendors.map((v) =>
    numbers ? labelWidth(meterLabel(v, display), font) : 0,
  );
  const widths = labels.map(
    (label) =>
      mark + (bar ? labelGap + bar : 0) + (numbers ? labelGap + label : 0),
  );
  const run = n
    ? (horizontal ? widths.reduce((a, b) => a + b, 0) : n * mark) +
      (n - 1) * gap
    : 32;
  const width = horizontal
    ? n
      ? left + run + right
      : 32
    : left + Math.max(mark, ...widths) + right;
  const height = horizontal
    ? padding.top + mark + padding.bottom
    : n
      ? padding.top + run + padding.bottom
      : 32;
  let along = horizontal ? left : padding.top;
  const centers = vendors.map((_, i) => {
    const center = along + mark / 2;
    along += (horizontal ? widths[i] : mark) + gap;
    return center;
  });
  return {
    width: width + (horizontal ? 0 : 3),
    height: height + (horizontal ? 3 : 0),
    gap,
    logo,
    mark,
    font,
    labelGap,
    label: Math.max(0, ...labels),
    bar,
    padding,
    radius: Math.min(Math.round(size * 0.73), Math.min(width, height) / 2),
    centers,
  };
}

export function parkedRect(
  screen: { width: number; height: number },
  strip: StripMetrics,
  config: HUDConfig,
): HUDRect {
  const edge = config.mode === "notch" ? "top" : config.edge;
  const width = Math.min(screen.width, strip.width),
    height = Math.min(screen.height, strip.height);
  const offset = config.mode === "notch" ? 0.5 : clamp(config.offset, 0, 1);
  return {
    x:
      edge === "left"
        ? 0
        : edge === "right"
          ? screen.width - width
          : (screen.width - width) * offset,
    y:
      edge === "top"
        ? 0
        : edge === "bottom"
          ? screen.height - height
          : (screen.height - height) * offset,
    width,
    height,
  };
}

export function cardRect(
  screen: { width: number; height: number },
  strip: HUDRect,
  edge: HUDEdge,
  size: { width: number; height: number },
  detached: boolean,
  along?: number,
  gap = 6,
): HUDRect {
  const width = Math.min(size.width, screen.width),
    height = Math.min(size.height, screen.height);
  const horizontal = edge === "top" || edge === "bottom";
  const center =
    along ??
    (horizontal ? strip.x + strip.width / 2 : strip.y + strip.height / 2);
  const x = clamp(Math.round(center - width / 2), 0, screen.width - width);
  const y = clamp(Math.round(center - height / 2), 0, screen.height - height);
  if (detached)
    return edge === "top"
      ? { x, y: strip.y + strip.height + gap, width, height }
      : edge === "bottom"
        ? { x, y: strip.y - gap - height, width, height }
        : edge === "left"
          ? { x: strip.x + strip.width + gap, y, width, height }
          : { x: strip.x - gap - width, y, width, height };
  return edge === "top"
    ? {
        x: clamp(
          strip.x + strip.width / 2 - width / 2,
          0,
          screen.width - width,
        ),
        y: 0,
        width,
        height,
      }
    : edge === "bottom"
      ? {
          x: clamp(
            strip.x + strip.width / 2 - width / 2,
            0,
            screen.width - width,
          ),
          y: screen.height - height,
          width,
          height,
        }
      : edge === "left"
        ? { x: 0, y, width, height }
        : { x: screen.width - width, y, width, height };
}

/** SVG equivalent of IslandShape.topPath. Content stays upright when this silhouette rotates. */
export function islandPath(
  width: number,
  height: number,
  flare: number,
  radius: number,
  edge: HUDEdge = "top",
): string {
  const horizontal = edge === "top" || edge === "bottom";
  const w = horizontal ? width : height,
    h = horizontal ? height : width;
  const t = Math.max(0, Math.min(flare, w / 2, h / 2));
  const b = Math.max(0, Math.min(radius, (w - t * 2) / 2, h - t));
  const point = (x: number, y: number) =>
    edge === "bottom"
      ? `${width - x} ${height - y}`
      : edge === "left"
        ? `${y} ${height - x}`
        : edge === "right"
          ? `${width - y} ${x}`
          : `${x} ${y}`;
  return `M${point(0, 0)} Q${point(t, 0)} ${point(t, t)} L${point(t, h - b)} A${b} ${b} 0 0 0 ${point(t + b, h)} L${point(w - t - b, h)} A${b} ${b} 0 0 0 ${point(w - t, h - b)} L${point(w - t, t)} Q${point(w - t, 0)} ${point(w, 0)} Z`;
}
