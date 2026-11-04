// Where the HUD's parts sit on its screen: the resting strip, the surface a
// drag grabs it by, and the card that opens from it.
import type { HUDConfig, HUDEdge } from "./model";
import { clamp, type HUDRect, type StripMetrics } from "./geometry";

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
