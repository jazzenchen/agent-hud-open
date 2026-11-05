import { useCallback, useEffect, useRef, useState } from "react";
import type { Approval } from "@agent-hud/protocol";
import type { EndSide } from "./end-geometry";
import {
  dropMessages,
  emptyEnds,
  fadeOldest,
  keepWaiting,
  parkCard,
  TAIL_FADE_MS,
  type ParkedCard,
} from "./end-queue";
import type { EndsLink } from "./ends-link";
import { useEndsSharing } from "./useEndsSharing";

/** The time left of the tail's round: one message goes every `ms`, the
 * oldest first, and the clock stops while `paused`. `cycle` changes
 * whenever a round starts, for the ring that shows it. An `ms` of 0 keeps
 * every message until the person lets it go. */
export function useTailFade(
  count: number,
  paused: boolean,
  fade: () => void,
  ms = TAIL_FADE_MS,
) {
  const [cycle, setCycle] = useState(0);
  const left = useRef(ms);
  const active = count > 0 && ms > 0;
  useEffect(() => {
    if (!active) return;
    left.current = ms;
    setCycle((n) => n + 1);
  }, [active, ms]);
  useEffect(() => {
    if (!active || paused) return;
    const start = performance.now();
    let fired = false;
    const timer = setTimeout(() => {
      fired = true;
      left.current = ms;
      fade();
      setCycle((n) => n + 1);
    }, left.current);
    return () => {
      clearTimeout(timer);
      if (!fired)
        left.current = Math.max(0, left.current - (performance.now() - start));
    };
  }, [active, paused, cycle]);
  return { cycle, paused: paused || !active, ms: ms > 0 ? ms : 0 };
}

export type EndFade = ReturnType<typeof useTailFade>;

/** The requests and messages parked at the dock's ends, which of them a
 * hovered end shows, and what the person can do with them there. With a
 * `link`, every screen's HUD keeps the same ends: what is parked, let go or
 * held open on one screen is so on all of them. */
export function useParkedEnds({
  requests,
  hovered,
  link,
  fadeMs,
}: {
  requests: Approval[];
  /** The end whose panel is open. */
  hovered: EndSide | null;
  link?: EndsLink;
  /** How often the tail lets its oldest message go; 0, never. */
  fadeMs?: number;
}) {
  const [parked, setParked] = useState(emptyEnds);
  const [picked, setPicked] = useState<string | null>(null);
  const waitingIds = requests.map((request) => request.id);
  useEffect(
    () => setParked((ends) => keepWaiting(ends, waitingIds)),
    [waitingIds.join("\0")],
  );
  const head = parked.head
    .map((id) => requests.find((request) => request.id === id))
    .filter((request): request is Approval => Boolean(request));
  const view: EndSide | null =
    hovered === "head" && head.length
      ? "head"
      : hovered === "tail" && parked.tail.length
        ? "tail"
        : null;
  useEffect(() => setPicked(null), [view]);
  const { share, claimed, heldElsewhere } = useEndsSharing(
    link,
    parked,
    setParked,
    view === "tail",
  );
  // Each screen lets its own oldest go; holding the same messages, and
  // stopping together, they let the same ones go.
  const fade = useTailFade(
    parked.tail.length,
    view === "tail" || heldElsewhere,
    () => setParked(fadeOldest),
    fadeMs,
  );
  const request =
    view === "head"
      ? (head.find((r) => r.id === picked) ?? head[0])
      : undefined;
  const message =
    view === "tail"
      ? (parked.tail.find((item) => item.id === picked) ?? parked.tail[0])
      : undefined;
  const park = useCallback(
    (card: ParkedCard) => {
      setParked((ends) => parkCard(ends, card));
      share({ type: "park", card });
    },
    [share],
  );
  const dropAll = useCallback(
    (ids: string[]) => {
      setParked((ends) => dropMessages(ends, ids));
      share({ type: "drop", ids });
    },
    [share],
  );
  const drop = useCallback((id: string) => dropAll([id]), [dropAll]);
  const clear = () => dropAll(parked.tail.map((item) => item.id));
  /** This screen shows these cards and will park them. */
  const claim = useCallback(
    (ids: string[]) => {
      if (ids.length) share({ type: "claim", ids });
    },
    [share],
  );
  return {
    view,
    head,
    tail: parked.tail,
    request,
    message,
    fade,
    park,
    pick: setPicked,
    drop,
    clear,
    claim,
    claimed,
  };
}

export type ParkedEnds = ReturnType<typeof useParkedEnds>;
