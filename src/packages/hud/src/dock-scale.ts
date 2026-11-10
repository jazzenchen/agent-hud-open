import type { StripMetrics } from "./geometry";

/** Review-approved whole-dock reduction. No CSS transform: hit regions, native
 * pointer anchors and the preview all consume these same scaled dimensions. */
export const DOCK_SCALE = 0.92;

export function scaleDockMetrics(
  metrics: StripMetrics,
  horizontal: boolean,
): StripMetrics {
  const scale = (value: number) => value * DOCK_SCALE;
  return {
    width: scale(metrics.width) + (horizontal ? 0 : 3),
    height: scale(metrics.height) + (horizontal ? 3 : 0),
    gap: scale(metrics.gap),
    logo: scale(metrics.logo),
    mark: scale(metrics.mark),
    font: scale(metrics.font),
    labelGap: scale(metrics.labelGap),
    label: scale(metrics.label),
    bar: scale(metrics.bar),
    radius: scale(metrics.radius),
    padding: {
      top: scale(metrics.padding.top),
      right: scale(metrics.padding.right),
      bottom: scale(metrics.padding.bottom),
      left: scale(metrics.padding.left),
    },
    centers: metrics.centers.map(scale),
  };
}
