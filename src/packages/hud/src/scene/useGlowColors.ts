import { useMemo } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { statusColor, type HUDVendor } from "../model";

/** The rim's colours, one per agent with a reading. Paused detection draws
 * the still grey rim, as no readings do. */
export function useGlowColors(vendors: HUDVendor[], snapshot: Snapshot) {
  const detectionPaused = Boolean(snapshot.status?.paused);
  return useMemo(
    () =>
      detectionPaused
        ? []
        : vendors
            .filter((vendor) => vendor.used != null)
            .map((vendor) => statusColor(vendor.level)),
    [vendors, detectionPaused],
  );
}
