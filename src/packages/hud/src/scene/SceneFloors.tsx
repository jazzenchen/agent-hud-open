import type { ReactNode } from "react";
import { position, type HUDRect } from "../geometry";

/** Parent-owned scenery behind the HUD, such as a desktop wallpaper. It never
 * takes the pointer. */
export function SceneBackdrop({ children }: { children?: ReactNode }) {
  if (!children) return null;
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        pointerEvents: "none",
        zIndex: -2,
      }}
    >
      {children}
    </div>
  );
}

/** The space a shrinking card left under the pointer, still the HUD's
 * (useHeldFrame's `heldFrame`). */
export function HoverFloor({
  frame,
  native,
  entered,
  domExited,
}: {
  frame: HUDRect | null;
  /** The host reports the pointer itself. */
  native: boolean;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
}) {
  if (!frame) return null;
  return (
    <div
      className="hud-hover-floor"
      aria-hidden="true"
      onPointerEnter={(e) => {
        if (!native) entered(e.altKey);
      }}
      onPointerLeave={domExited}
      style={{ ...position(frame), zIndex: 1, background: "transparent" }}
    />
  );
}
