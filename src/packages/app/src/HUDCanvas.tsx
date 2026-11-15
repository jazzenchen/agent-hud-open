import type { ReactNode } from "react";
import type { HUDEdge } from "@agent-hud/hud/model";
import type { NativeHUDGeometry } from "./lib/use-hud-geometry";
import { useHUDDepth } from "./lib/use-hud-depth";

/** The HUD laid out on its whole canvas, held against its edge. The window
 * may show only the part next to the edge (`useHUDDepth`); a window that
 * grows or shrinks never moves what is drawn against the edge. */
export function HUDCanvas({
  geometry,
  edge,
  children,
}: {
  geometry: NativeHUDGeometry | null;
  edge: HUDEdge;
  children: ReactNode;
}) {
  useHUDDepth(edge, Boolean(geometry));
  if (!geometry) return <>{children}</>;
  return (
    <div
      className="hud-canvas"
      style={{
        position: "fixed",
        width: geometry.canvas.width,
        height: geometry.canvas.height,
        ...(edge === "bottom" ? { bottom: 0 } : { top: 0 }),
        ...(edge === "right" ? { right: 0 } : { left: 0 }),
      }}
    >
      {children}
    </div>
  );
}
