import { useCallback, useMemo, useRef, useState } from "react";
import type { HUDRect } from "../geometry";
import type { EndSide } from "./end-geometry";
import { endFor, type ParkedCard } from "./end-queue";

/** A card on its way from beside its agent's mark to its end. */
export interface EndFlightPlan {
  id: string;
  side: EndSide;
  from: HUDRect;
  source: string;
}

/** A flight lands by itself after this long, should its animation never end. */
const LANDS_WITHIN_MS = 1500;

/** The scene's side of the dock's ends: the end the pointer is on, the cards
 * flying to them, and where the card on show is, for the next one to park. */
export function useDockEnds(reduceMotion: boolean) {
  const [hovered, setHovered] = useState<EndSide | null>(null);
  const [flights, setFlights] = useState<EndFlightPlan[]>([]);
  /** The card on show beside a mark, kept by the layer that draws the ends. */
  const cardFrame = useRef<HUDRect | null>(null);
  const still = useRef(reduceMotion);
  still.current = reduceMotion;
  const land = useCallback(
    (id: string) => setFlights((list) => list.filter((f) => f.id !== id)),
    [],
  );
  const onPark = useCallback(
    (card: ParkedCard, source: string) => {
      const from = cardFrame.current;
      if (!from || still.current) return;
      const flight = { id: card.id, side: endFor(card), from, source };
      setFlights((list) => [...list.filter((f) => f.id !== card.id), flight]);
      setTimeout(() => land(card.id), LANDS_WITHIN_MS);
    },
    [land],
  );
  const arriving = useMemo(
    () => new Set(flights.map((flight) => flight.id)),
    [flights],
  );
  return { hovered, setHovered, flights, onPark, land, arriving, cardFrame };
}

export type DockEndsState = ReturnType<typeof useDockEnds>;
