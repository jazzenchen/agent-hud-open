import { useEffect, useState } from "react";
import type { HUDRect } from "../geometry";
import type { HUDEdge } from "../model";
import {
  canRetractAtEdge,
  EDGE_RETRACT_DELAY_MS,
  retractedStripFrame,
  type CompactStripMetrics,
  type EdgeRetractGate,
} from "./edge-retract";

/** Manages auto-retracting a docked Simple strip into a compact Dynamic-style
 * edge strip when idle, and waking immediately on hover, alert, open, or drag. */
export function useEdgeRetract(
  gate: EdgeRetractGate,
  strip: HUDRect,
  edge: HUDEdge,
  vendorCount: number,
  compact?: CompactStripMetrics,
) {
  const allowed = canRetractAtEdge(gate);
  const [retracted, setRetracted] = useState(false);

  useEffect(() => {
    if (!allowed) {
      setRetracted(false);
      return;
    }
    const timer = setTimeout(() => setRetracted(true), EDGE_RETRACT_DELAY_MS);
    return () => clearTimeout(timer);
  }, [allowed]);

  return {
    retracted,
    effectiveStrip: retracted
      ? retractedStripFrame(strip, edge, vendorCount, compact)
      : strip,
  };
}
