// The dock's drawer. A Simple dock that has nothing to answer for folds to a
// thin bar where its middle was, the way a drawer closes into its edge; the
// pointer on that bar unfolds it, and a click asks for the panel. A dock with
// a window running out wears that level on the bar, so it still shows closed.
import { useEffect, useState, type CSSProperties } from "react";
import { position, type HUDRect, type StripMetrics } from "../geometry";
import { statusColor, type HUDConfig, type HUDVendor } from "../model";
import { useT } from "@agent-hud/ui/format-context";
import { drawerHandleRect } from "./drawer-geometry";

/** The tightest level among the marks, when one wants attention. */
function wornLevel(vendors: HUDVendor[]): string | null {
  if (vendors.some((vendor) => vendor.level === "critical"))
    return statusColor("critical");
  if (vendors.some((vendor) => vendor.level === "warning"))
    return statusColor("warning");
  return null;
}

/** The dock folded: the bar that stands in for it, taking the pointer and
 * the click the dock itself would. It stays in place, clear of the pointer,
 * while the dock is out, so the host always has something to hit at rest. */
export function DrawerHandle({
  config,
  folded,
  strip,
  metrics,
  vendors,
  onEnter,
  onOpen,
}: {
  config: HUDConfig;
  folded: boolean;
  strip: HUDRect;
  metrics: StripMetrics;
  vendors: HUDVendor[];
  onEnter: () => void;
  onOpen: () => void;
}) {
  const t = useT();
  if (!config.drawer || config.mode !== "simple") return null;
  const rect = drawerHandleRect(config.edge, strip, metrics);
  const worn = wornLevel(vendors);
  // Frosted, not painted: a pane of the HUD's own text colour over the
  // desktop, with a lit edge and a soft shadow, as a phone's bar floats.
  const fillColor = worn
    ? `color-mix(in srgb, ${worn} 58%, transparent)`
    : "color-mix(in srgb, currentColor 26%, transparent)";
  const ring = worn
    ? `color-mix(in srgb, ${worn} 72%, transparent)`
    : "color-mix(in srgb, currentColor 34%, transparent)";
  const lit = "color-mix(in srgb, currentColor 30%, transparent)";
  return (
    <button
      type="button"
      className="hud-drawer-handle"
      aria-label={t("Hover to expand Agent HUD")}
      aria-hidden={!folded}
      tabIndex={folded ? 0 : -1}
      onPointerEnter={() => onEnter()}
      onClick={() => onOpen()}
      style={
        {
          ...position(rect),
          zIndex: 3,
          display: "block",
          padding: 0,
          border: 0,
          color: "inherit",
          borderRadius: Math.min(rect.width, rect.height) / 2,
          "--handle-fill": fillColor,
          boxShadow: `inset 0 0 0 0.5px ${ring}, inset 0 1px 0 ${lit}, 0 2px 8px rgba(0, 0, 0, 0.26)`,
          backdropFilter: "blur(12px) saturate(1.6)",
          WebkitBackdropFilter: "blur(12px) saturate(1.6)",
          opacity: folded ? 1 : 0,
          pointerEvents: folded ? undefined : "none",
          cursor: "pointer",
        } as CSSProperties
      }
    />
  );
}

/** Whether the dock is folded now: it unfolds the moment anything is out or
 * the pointer is on it, and folds a beat after all of that lets go. Only a
 * Simple dock has a drawer; every other mode keeps its strip out. */
export function useDrawerFold(
  config: HUDConfig,
  state: { open: boolean; cardVisible: boolean; pointerInside: boolean },
): boolean {
  const resting = !state.open && !state.cardVisible && !state.pointerInside;
  const delay = Math.max(0, config.collapseDelayMs);
  const [folded, setFolded] = useState(resting);
  useEffect(() => {
    if (!resting) {
      setFolded(false);
      return;
    }
    const timer = setTimeout(() => setFolded(true), delay);
    return () => clearTimeout(timer);
  }, [resting, delay]);
  return config.drawer && config.mode === "simple" ? folded : false;
}
