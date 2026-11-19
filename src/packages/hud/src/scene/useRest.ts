import { useEffect, useState } from "react";
import type { HUDRect, StripMetrics } from "../geometry";
import type { HUDConfig, HUDEdge, HUDVendor } from "../model";
import {
  barPaint,
  foldStyle,
  logoStrip,
  mayRest,
  restShape,
  restZone,
  type Rest,
  type RestGate,
} from "./rest";

/** This screen's Simple dock at rest, or nothing for a dock that never
 * folds. It folds a collapse delay after the last thing that needs it lets
 * go, and is back out the moment anything needs it again. */
export function useRest({
  config,
  gate,
  strip,
  edge,
  screen,
  metrics,
  vendors,
  light,
  reduceMotion,
}: {
  config: HUDConfig;
  gate: RestGate;
  strip: HUDRect;
  edge: HUDEdge;
  screen: { width: number; height: number };
  metrics: StripMetrics;
  vendors: HUDVendor[];
  light: boolean;
  reduceMotion: boolean;
}): Rest | undefined {
  const style = foldStyle(config);
  const resting = style !== null && mayRest(gate);
  const delay = Math.max(0, config.collapseDelayMs);
  // A HUD that starts with nothing to show starts folded.
  const [folded, setFolded] = useState(resting);
  useEffect(() => {
    if (!resting) {
      setFolded(false);
      return;
    }
    const timer = setTimeout(() => setFolded(true), delay);
    return () => clearTimeout(timer);
  }, [resting, delay]);
  if (!style) return undefined;
  const shape = restShape(
    style,
    config,
    strip,
    edge,
    screen,
    metrics,
    vendors.length,
  );
  return {
    style,
    edge,
    folded: folded && resting,
    shape,
    zone: restZone(shape, edge),
    logos: logoStrip(metrics),
    bar:
      style === "bar"
        ? barPaint(
            config,
            vendors.map((vendor) => vendor.level),
            light,
          )
        : null,
    reduceMotion,
  };
}
