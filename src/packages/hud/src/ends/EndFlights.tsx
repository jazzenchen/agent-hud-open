import { useLayoutEffect, useRef } from "react";
import { HUDArtwork } from "../HUDArtwork";
import type { HUDRect } from "../geometry";
import type { HUDEdge } from "../model";
import type { EndSide } from "./end-geometry";
import type { EndFlightPlan } from "./useDockEnds";

/** How far a flight bows away from the edge, and how long it takes. */
const BOW = 18;
const FLIGHT_MS = 620;

const centre = (rect: HUDRect) => ({
  x: rect.x + rect.width / 2,
  y: rect.y + rect.height / 2,
});

/** One card on its way: a circle with its agent's mark, drawn where it lands
 * and moved there by transform and opacity alone. Web Animations hand those
 * to the compositor, so the flight keeps its pace while the page is busy
 * with the card it left. */
function EndFlight({
  flight,
  to,
  inward,
  ground,
  outline,
  light,
  land,
}: {
  flight: EndFlightPlan;
  to: HUDRect;
  inward: { x: number; y: number };
  ground: string;
  outline: string;
  light: boolean;
  land: (id: string) => void;
}) {
  const chip = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = chip.current;
    if (!element?.animate) return land(flight.id);
    const from = centre(flight.from);
    const at = centre(to);
    const dx = from.x - at.x,
      dy = from.y - at.y;
    const start = Math.min(
      1.2,
      Math.max(0.6, Math.min(flight.from.height, 34) / to.width),
    );
    const mid = (start + 1) / 2;
    const animation = element.animate(
      [
        {
          transform: `translate(${dx}px, ${dy}px) scale(${start})`,
          opacity: 0,
        },
        {
          transform: `translate(${dx / 2 + inward.x}px, ${dy / 2 + inward.y}px) scale(${mid})`,
          opacity: 1,
          offset: 0.45,
        },
        { transform: "translate(0px, 0px) scale(1)", opacity: 0.85 },
      ],
      {
        duration: FLIGHT_MS,
        easing: "cubic-bezier(0.45, 0, 0.2, 1)",
        fill: "forwards",
      },
    );
    animation.onfinish = () => land(flight.id);
    return () => animation.cancel();
  }, [flight.id]);
  return (
    <div
      ref={chip}
      aria-hidden="true"
      style={{
        position: "absolute",
        left: to.x,
        top: to.y,
        width: to.width,
        height: to.height,
        zIndex: 4,
        pointerEvents: "none",
        opacity: 0,
        willChange: "transform, opacity",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: "50%",
        background: ground,
        boxShadow: `inset 0 0 0 1px ${outline}`,
      }}
    >
      <HUDArtwork
        source={flight.source}
        size={Math.round(to.width * 0.5)}
        light={light}
      />
    </div>
  );
}

/** Cards on their way to the dock's ends: each leaves the card's place as a
 * small circle with its agent's mark, slides along the dock in a low arc
 * and settles into its end, which counts it once it lands. */
export function EndFlights({
  flights,
  rects,
  edge,
  ground,
  outline,
  light,
  land,
}: {
  flights: EndFlightPlan[];
  rects: Record<EndSide, HUDRect>;
  edge: HUDEdge;
  /** The chip's fill: the HUD's ground, the notch's black in its row. */
  ground: string;
  outline: string;
  light: boolean;
  land: (id: string) => void;
}) {
  const inward = {
    x: edge === "left" ? BOW : edge === "right" ? -BOW : 0,
    y: edge === "top" ? BOW : edge === "bottom" ? -BOW : 0,
  };
  return (
    <>
      {flights.map((flight) => (
        <EndFlight
          key={flight.id}
          flight={flight}
          to={rects[flight.side]}
          inward={inward}
          ground={ground}
          outline={outline}
          light={light}
          land={land}
        />
      ))}
    </>
  );
}
