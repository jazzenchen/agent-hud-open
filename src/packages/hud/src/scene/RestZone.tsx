import type { PointerEvent as ReactPointerEvent } from "react";
import { position } from "../geometry";
import type { Rest } from "./rest";
import { innerCorners, intoEdge, restTransition } from "./rest-style";
import type { HUDNativePointer } from "./useHUDInteraction";

/** What a resting Simple dock leaves at its edge: a zone flush against it
 * that the pointer finds, drawing the bar when the dock rests as one. It
 * stays, under the dock, while the dock is out, so the pointer never falls
 * between the two; the host counts it as the HUD's (`hud_report_regions`),
 * and lets clicks through everywhere else the folded dock was. */
export function RestZone({
  rest,
  nativePointer,
  entered,
  domExited,
  considerDrag,
}: {
  rest: Rest;
  nativePointer?: HUDNativePointer;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
  considerDrag: (event: ReactPointerEvent<HTMLDivElement>) => void;
}) {
  const { zone, shape, bar, folded, edge } = rest;
  const depth = Math.min(shape.width, shape.height);
  return (
    <div
      className="hud-rest-zone"
      aria-hidden="true"
      data-folded={folded || undefined}
      style={{ ...position(zone), zIndex: 1, userSelect: "none" }}
      onPointerEnter={(event) => {
        if (!nativePointer) entered(event.altKey);
      }}
      onPointerLeave={domExited}
      onPointerDownCapture={considerDrag}
    >
      {bar && (
        <span
          className="hud-rest-bar"
          style={{
            position: "absolute",
            left: shape.x - zone.x,
            top: shape.y - zone.y,
            width: shape.width,
            height: shape.height,
            borderRadius: innerCorners(edge, depth),
            backgroundColor: bar.fill,
            boxShadow: `0 0 0 0.5px ${bar.edge}`,
            // Out of the edge as the dock folds, back into it as it unfolds.
            opacity: folded ? 1 : 0,
            transform: folded ? "none" : intoEdge(edge, depth + 1),
            transition: restTransition(
              ["opacity", "transform", "background-color"],
              rest.reduceMotion,
            ),
            pointerEvents: "none",
          }}
        />
      )}
    </div>
  );
}
