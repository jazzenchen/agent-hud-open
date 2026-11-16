// Outlines of the HUD's own surfaces for the host's glass, as path commands in
// the page's points, top-left origin: [0, x, y] move, [1, x, y] line,
// [2, cx, cy, x, y] quadratic, [3, c1x, c1y, c2x, c2y, x, y] cubic, [4] close.

export type GlassCommand = number[];

/** Whether the HUD offers glass at all. Off (2026-11-16, the owner is still
 * deciding): every look draws solid, Settings shows no material, the HUD
 * tools take none, and the host's glass is never asked for. A saved
 * `hudMaterial` stays as it is. */
export const HUD_GLASS = false;

/** Whether a look draws its surfaces on glass. */
export function glassLook(look: { hudMaterial?: string }): boolean {
  return HUD_GLASS && look.hudMaterial === "glass";
}

/** A quarter circle's control-point share as a cubic. */
const KAPPA = 0.5523;

/** How much of the ground covers what is behind it. Over glass the ground is
 * a tint that keeps the look's tone and its text legible on any desktop (a
 * 70% cover stays dark over white); the transparency thins it. */
export function groundAlpha(look: {
  solidTransparency: number;
  hudMaterial?: string;
}) {
  return glassLook(look)
    ? 0.7 - 0.45 * look.solidTransparency
    : 1 - look.solidTransparency;
}

/** The notch island's outline at (x, y), as `islandPath` draws it at the top
 * edge: shoulders flaring into the screen's edge, round lower corners. */
export function islandOutline(
  x: number,
  y: number,
  width: number,
  height: number,
  flare: number,
  radius: number,
): GlassCommand[] {
  const t = Math.max(0, Math.min(flare, width / 2, height / 2));
  const b = Math.max(0, Math.min(radius, (width - t * 2) / 2, height - t));
  const k = b * KAPPA;
  const w = width,
    h = height;
  return [
    [0, x, y],
    [2, x + t, y, x + t, y + t],
    [1, x + t, y + h - b],
    [3, x + t, y + h - b + k, x + t + b - k, y + h, x + t + b, y + h],
    [1, x + w - t - b, y + h],
    [
      3,
      x + w - t - b + k,
      y + h,
      x + w - t,
      y + h - b + k,
      x + w - t,
      y + h - b,
    ],
    [1, x + w - t, y + t],
    [2, x + w - t, y, x + w, y],
    [4],
  ];
}

/** A rounded box's outline. */
export function boxOutline(
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): GlassCommand[] {
  const r = Math.max(0, Math.min(radius, width / 2, height / 2));
  const k = r * KAPPA;
  const right = x + width,
    bottom = y + height;
  return [
    [0, x + r, y],
    [1, right - r, y],
    [3, right - r + k, y, right, y + r - k, right, y + r],
    [1, right, bottom - r],
    [3, right, bottom - r + k, right - r + k, bottom, right - r, bottom],
    [1, x + r, bottom],
    [3, x + r - k, bottom, x, bottom - r + k, x, bottom - r],
    [1, x, y + r],
    [3, x, y + r - k, x + r - k, y, x + r, y],
    [4],
  ];
}
