import type { BotBadge, BotEvent, BotState } from "./bot-state";

// What the director in Core (crates/hud/src/bots.rs) hands each mark, and
// DockMemory. Director times are UTC seconds; a host converts a delivered cue
// to its own monotonic animation clock once.
export interface Direction {
  state: BotState;
  cue: { event: BotEvent; at: number } | null;
}
export class DockMemory {
  private handed = new Map<string, Direction["cue"]>();
  private replays = new Map<string, Direction["cue"]>();
  cue(
    key: string,
    cue: Direction["cue"],
    badge: BotBadge | null,
    now: number,
  ): Direction["cue"] {
    if (!cue) return null;
    const before = this.handed.get(key);
    if (before?.at === cue.at && before.event === cue.event)
      return this.replays.get(key) ?? cue;
    this.handed.set(key, cue);
    this.replays.delete(key);
    if (
      now - cue.at > 3 &&
      ((cue.event === "failed" && badge === "failed") ||
        (cue.event === "limit-hit" && badge === "limited"))
    ) {
      const replay = { event: cue.event, at: now };
      this.replays.set(key, replay);
      return replay;
    }
    return cue;
  }
}
export function monotonicDirection(
  direction: Direction,
  epochOffset: number,
): Direction {
  return {
    ...direction,
    cue: direction.cue
      ? { ...direction.cue, at: direction.cue.at - epochOffset }
      : null,
  };
}
