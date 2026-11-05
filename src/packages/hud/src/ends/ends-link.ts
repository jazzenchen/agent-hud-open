// How the HUDs of several screens keep one set of dock ends. Each screen's
// HUD is a window of its own; the host's app passes a link between them,
// and every window applies what the others send. The link carries only what
// happened; each window keeps its own copy, and the copies agree.
import type { EndQueues, ParkedCard } from "./end-queue.ts";

export type EndsOp =
  /** A card had its moment and parks at its end. */
  | { type: "park"; card: ParkedCard }
  /** Messages the person let go: an x, or everything at "Clear all". */
  | { type: "drop"; ids: string[] }
  /** The screen under the pointer shows these cards and will park them; the
   * other screens leave them to it. */
  | { type: "claim"; ids: string[] }
  /** A tail's panel is open (or closed again) on the sender's screen: the
   * tail's clock stops everywhere while one is. */
  | { type: "hold"; on: boolean }
  /** A window that just started asks what waits at the ends. */
  | { type: "hello" }
  /** What waits at the ends, as an answer to "hello". */
  | { type: "state"; ends: EndQueues };

export interface EndsLink {
  /** Sends to every other window. */
  send(op: EndsOp): void;
  /** Hears the other windows, with the sender's id; returns the way to stop. */
  listen(handle: (op: EndsOp, from: string) => void): () => void;
}

/** How long a screen waits for the pointer's screen to claim a new card
 * before it parks the card itself (the pointer is on a screen with no HUD). */
export const CLAIM_GRACE_MS = 1500;
/** A tail held open on another screen sends this often while it stays open;
 * a hold not heard of for twice as long has ended (its window went away). */
export const HOLD_BEAT_MS = 10_000;
