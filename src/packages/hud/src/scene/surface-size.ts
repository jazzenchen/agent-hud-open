import type { HUDConfig } from "../model";

export const SURFACE_BORDER_PX = 2;

/** The HUD's ground: its fill, and the outline and shadow that belong to it.
 * Transparency runs it from opaque (`alpha` 1) to fully clear, the outline
 * and shadow fading with it; logos, bots and text are drawn over it at full
 * strength. */
export function hudGround(light: boolean, alpha: number) {
  return {
    fill: light ? `rgba(245,245,247,${alpha})` : `rgba(0,0,0,${alpha})`,
    outline: light
      ? `rgba(0,0,0,${0.106 * alpha})`
      : `rgba(255,255,255,${0.075 * alpha})`,
    shadow: `0 9px 30px rgba(0,0,0,${0.133 * alpha})`,
  };
}

export function isSurfaceBordered(
  mode: HUDConfig["mode"],
  detached: boolean,
  simplePeek: boolean,
): boolean {
  return (detached || mode === "simple") && !simplePeek;
}

export function peekBubbleHeight(
  vendor: { quotas: readonly unknown[]; balance: string | null },
  scale: number,
  bordered: boolean,
): number {
  const emptyFallback = !vendor.quotas.length && !vendor.balance ? 1 : 0;
  const rows =
    1 + vendor.quotas.length + (vendor.balance ? 1 : 0) + emptyFallback;
  return (20 + rows * 18) * scale + (bordered ? SURFACE_BORDER_PX : 0);
}

export function expandedPanelHeight(
  contentHeight: number,
  screenHeight: number,
  bordered: boolean,
): number {
  const borderAllowance = bordered ? SURFACE_BORDER_PX : 0;
  return Math.max(
    80,
    Math.min(contentHeight + borderAllowance, screenHeight - 80),
  );
}

export function bodyMaxHeight(
  frameHeight: number,
  padding: { top: number; bottom: number },
  bordered: boolean,
): number {
  return Math.max(
    40,
    frameHeight -
      (bordered ? SURFACE_BORDER_PX : 0) -
      padding.top -
      padding.bottom,
  );
}
