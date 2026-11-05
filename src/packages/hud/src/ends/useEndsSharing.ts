import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { dropMessages, mergeEnds, parkCard, type EndQueues } from "./end-queue";
import { HOLD_BEAT_MS, type EndsLink, type EndsOp } from "./ends-link";

/** The most claimed ids a screen remembers; past it, it starts afresh. */
const CLAIMS_KEPT = 256;

/** This screen's side of the shared ends: it applies what the other screens
 * send, answers a newcomer with what waits here, tells the others while its
 * tail's panel is open, and knows which cards the pointer's screen claimed.
 * Without a link (one window, a picture) it shares nothing. */
export function useEndsSharing(
  link: EndsLink | undefined,
  parked: EndQueues,
  setParked: Dispatch<SetStateAction<EndQueues>>,
  holding: boolean,
) {
  const latest = useRef(parked);
  latest.current = parked;
  const claims = useRef(new Set<string>());
  // When each other screen last said its tail's panel is open.
  const [holds, setHolds] = useState<Record<string, number>>({});
  useEffect(() => {
    if (!link) return;
    const stop = link.listen((op, from) => {
      if (op.type === "park") setParked((ends) => parkCard(ends, op.card));
      else if (op.type === "drop")
        setParked((ends) => dropMessages(ends, op.ids));
      else if (op.type === "state")
        setParked((ends) => mergeEnds(ends, op.ends));
      else if (op.type === "hello")
        link.send({ type: "state", ends: latest.current });
      else if (op.type === "claim") {
        if (claims.current.size > CLAIMS_KEPT) claims.current.clear();
        for (const id of op.ids) claims.current.add(id);
      } else if (op.type === "hold")
        setHolds(({ [from]: _, ...rest }) =>
          op.on ? { ...rest, [from]: Date.now() } : rest,
        );
    });
    link.send({ type: "hello" });
    return stop;
  }, [link]);
  useEffect(() => {
    if (!link) return;
    link.send({ type: "hold", on: holding });
    if (!holding) return;
    const beat = setInterval(
      () => link.send({ type: "hold", on: true }),
      HOLD_BEAT_MS,
    );
    return () => clearInterval(beat);
  }, [link, holding]);
  // A hold not heard of again in time has ended: its window went away.
  useEffect(() => {
    const times = Object.values(holds);
    if (!times.length) return;
    const expires = Math.min(...times) + HOLD_BEAT_MS * 2;
    const timer = setTimeout(
      () =>
        setHolds((current) =>
          Object.fromEntries(
            Object.entries(current).filter(
              ([, at]) => Date.now() - at < HOLD_BEAT_MS * 2,
            ),
          ),
        ),
      Math.max(0, expires - Date.now()) + 50,
    );
    return () => clearTimeout(timer);
  }, [holds]);
  const share = useCallback((op: EndsOp) => link?.send(op), [link]);
  const claimed = useCallback((id: string) => claims.current.has(id), []);
  return {
    share,
    claimed,
    heldElsewhere: Object.keys(holds).length > 0,
  };
}
