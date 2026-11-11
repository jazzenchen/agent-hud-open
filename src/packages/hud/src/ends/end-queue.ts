// What waits at the dock's ends. A card that has had its moment parks: a
// request in the head, until it is answered; a message in the tail, which
// lets one go every TAIL_FADE_MS, the oldest first.
import type { IslandEvent } from "@agent-hud/protocol";
import type { EndSide } from "./end-geometry.ts";

/** How long a card shows beside its agent's mark before it parks, unless the
 * pointer or the keyboard is on the HUD. */
export const PARK_AFTER_MS = { request: 5000, news: 3000 } as const;
/** The tail lets one message go this often. */
export const TAIL_FADE_MS = 5000;
/** The most messages the tail keeps; past it the oldest go at once. */
export const TAIL_LIMIT = 32;

export interface TailItem {
  id: string;
  at: string;
  event: IslandEvent;
}

export interface EndQueues {
  /** Request ids, in the order they parked. */
  head: string[];
  /** Messages, newest first. */
  tail: TailItem[];
}

export const emptyEnds: EndQueues = { head: [], tail: [] };

/** A card that had its moment, put away at its end. */
export type ParkedCard =
  | { type: "request"; id: string }
  | { type: "news"; id: string; at: string; event: IslandEvent };

export const endFor = (card: ParkedCard): EndSide =>
  card.type === "request" ? "head" : "tail";

export function parkCard(ends: EndQueues, card: ParkedCard): EndQueues {
  if (card.type === "request")
    return ends.head.includes(card.id)
      ? ends
      : { ...ends, head: [...ends.head, card.id] };
  if (ends.tail.some((item) => item.id === card.id)) return ends;
  const item = { id: card.id, at: card.at, event: card.event };
  return { ...ends, tail: [item, ...ends.tail].slice(0, TAIL_LIMIT) };
}

/** The tail's oldest message goes. */
export function fadeOldest(ends: EndQueues): EndQueues {
  return ends.tail.length ? { ...ends, tail: ends.tail.slice(0, -1) } : ends;
}

export function dropMessages(
  ends: EndQueues,
  ids: readonly string[],
): EndQueues {
  const tail = ends.tail.filter((item) => !ids.includes(item.id));
  return tail.length === ends.tail.length ? ends : { ...ends, tail };
}

/** This screen's ends with what another screen says waits there: every
 * request and message either knows of, messages newest first. */
export function mergeEnds(ends: EndQueues, other: EndQueues): EndQueues {
  const head = [
    ...ends.head,
    ...other.head.filter((id) => !ends.head.includes(id)),
  ];
  const known = new Set(ends.tail.map((item) => item.id));
  const extra = other.tail.filter((item) => !known.has(item.id));
  if (!extra.length && head.length === ends.head.length) return ends;
  const tail = [...ends.tail, ...extra]
    .sort((a, b) => Date.parse(b.at) - Date.parse(a.at))
    .slice(0, TAIL_LIMIT);
  return { head, tail };
}

/** The head keeps only the requests still waiting; one answered anywhere
 * leaves it. */
export function keepWaiting(
  ends: EndQueues,
  waiting: readonly string[],
): EndQueues {
  const head = ends.head.filter((id) => waiting.includes(id));
  return head.length === ends.head.length ? ends : { ...ends, head };
}
