import type { StripMetrics } from "./geometry";

/** Review-approved whole-dock reduction. No CSS transform: hit regions, native
 * pointer anchors and the preview all consume these same scaled dimensions. */
export const DOCK_SCALE = 0.92;

export const dockScale = (value: number) => Math.max(0.6, Math.min(1.6, value));
export const dockFontWeight = (value: number) =>
  Math.round(Math.max(350, Math.min(650, 400 * Math.sqrt(dockScale(value)))));

export function scaleDockMetrics(
  metrics: StripMetrics,
  horizontal: boolean,
  multiplier = 1,
  height = 1,
  roundness = 1,
): StripMetrics {
  const scale = (value: number) => value * DOCK_SCALE * dockScale(multiplier);
  // Change the dock's thickness, with enough room for the original content.
  // The transparent native edge margin remains outside this shape.
  const thickness = horizontal ? metrics.height : metrics.width;
  const least = horizontal
    ? metrics.mark
    : metrics.width - metrics.padding.left - metrics.padding.right;
  const cross = Math.max(
    least,
    thickness * Math.max(0.75, Math.min(1.75, height)),
  );
  const inset = (cross - thickness) / 2;
  return {
    width: scale(horizontal ? metrics.width : cross) + (horizontal ? 0 : 3),
    height: scale(horizontal ? cross : metrics.height) + (horizontal ? 3 : 0),
    gap: scale(metrics.gap),
    logo: scale(metrics.logo),
    mark: scale(metrics.mark),
    font: scale(metrics.font),
    labelGap: scale(metrics.labelGap),
    label: scale(metrics.label),
    ...(metrics.labels && { labels: metrics.labels.map(scale) }),
    bar: scale(metrics.bar),
    radius: scale(
      (Math.min(metrics.width, horizontal ? cross : metrics.height, cross) /
        2) *
        Math.max(0, Math.min(1, roundness)),
    ),
    padding: {
      top: scale(metrics.padding.top + (horizontal ? inset : 0)),
      right: scale(metrics.padding.right + (horizontal ? 0 : inset)),
      bottom: scale(metrics.padding.bottom + (horizontal ? inset : 0)),
      left: scale(metrics.padding.left + (horizontal ? 0 : inset)),
    },
    centers: metrics.centers.map(scale),
  };
}
