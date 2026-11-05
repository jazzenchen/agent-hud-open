import { motion } from "motion/react";
import { HUDArtwork } from "../HUDArtwork";
import type { HUDRect } from "../geometry";
import type { HUDEdge } from "../model";
import type { EndSide } from "./end-geometry";
import type { EndFlightPlan } from "./useDockEnds";

/** How far a flight bows away from the edge, and how long it takes. */
const BOW = 18;
const FLIGHT_SECONDS = 0.62;

const centre = (rect: HUDRect) => ({
  x: rect.x + rect.width / 2,
  y: rect.y + rect.height / 2,
});

/** Cards on their way to the dock's ends: each leaves the card's place as a
 * small chip with its agent's mark, slides along the dock in a low arc and
 * shrinks into its end, which counts it once it lands. */
export function EndFlights({
  flights,
  rects,
  edge,
  thick,
  ground,
  outline,
  light,
  land,
}: {
  flights: EndFlightPlan[];
  rects: Record<EndSide, HUDRect>;
  edge: HUDEdge;
  thick: number;
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
      {flights.map((flight) => {
        const to = rects[flight.side];
        const from = centre(flight.from);
        const at = centre(to);
        const size = Math.min(Math.max(flight.from.height, thick), thick * 1.4);
        const width = Math.min(flight.from.width, size * 2.4);
        const mid = {
          x: (from.x + at.x) / 2 + inward.x,
          y: (from.y + at.y) / 2 + inward.y,
          width: (width + to.width) / 2,
          height: (size + to.height) / 2,
        };
        return (
          <motion.div
            key={flight.id}
            aria-hidden="true"
            initial={{
              left: from.x - width / 2,
              top: from.y - size / 2,
              width,
              height: size,
              opacity: 0,
            }}
            animate={{
              left: [from.x - width / 2, mid.x - mid.width / 2, to.x],
              top: [from.y - size / 2, mid.y - mid.height / 2, to.y],
              width: [width, mid.width, to.width],
              height: [size, mid.height, to.height],
              opacity: [0, 1, 0.85],
            }}
            transition={{
              duration: FLIGHT_SECONDS,
              ease: [0.45, 0, 0.2, 1],
              times: [0, 0.45, 1],
            }}
            onAnimationComplete={() => land(flight.id)}
            style={{
              position: "absolute",
              zIndex: 4,
              pointerEvents: "none",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 999,
              background: ground,
              boxShadow: `inset 0 0 0 1px ${outline}, 0 4px 12px rgba(0,0,0,0.18)`,
            }}
          >
            <HUDArtwork
              source={flight.source}
              size={Math.round(thick * 0.55)}
              light={light}
            />
          </motion.div>
        );
      })}
    </>
  );
}
