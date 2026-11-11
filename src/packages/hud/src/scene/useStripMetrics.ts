import { useMemo } from "react";
import type { QuotaDisplay } from "@agent-hud/ui/format";
import { stripMetrics } from "../geometry";
import type { HUDConfig, HUDVendor } from "../model";

/** The resting strip's measures, worked out again only when something they
 * depend on changes. */
export function useStripMetrics(
  config: HUDConfig,
  vendors: HUDVendor[],
  display?: QuotaDisplay,
) {
  return useMemo(
    () => stripMetrics(config, vendors, display),
    [
      vendors,
      display,
      config.mode,
      config.edge,
      config.simpleSize,
      config.simpleGapScale,
      config.ringWidth,
      config.showsNumbers,
      config.simpleStyle,
      config.compactDock,
      config.dockQuotaWarningThreshold,
      config.panelLayout,
      config.notchWidth,
      config.notchHeight,
      config.hasNotch,
    ],
  );
}
