// A host's flight between two agents' marks: which one is in the air and how
// many ride along with it.

/** One flight the host asked for (the host's `Flight`). */
export interface HUDFlight {
  /** Flights between the same two ends share a lane. */
  lane: string;
  from: string;
  to: string;
  /** Around the notch, the sender stands on the left; otherwise on the right. */
  fromLeft: boolean;
  /** The receiver appears first, then catches it. */
  arrives: boolean;
  /** It falls halfway and the sender is left puzzled. */
  falls: boolean;
}
/** What the host sends each HUD: a flight, its arrival order and when it
 * came (`performance.now()`): a scene that mounts later does not replay it. */
export interface HUDFlightCommand {
  flight: HUDFlight;
  sequence: number;
  at: number;
}
/** How old a flight may be when its scene first sees it. */
export const FLIGHT_FRESHNESS_MS = 2000;
/** The flight in the air and how many it carries (one, or more that came meanwhile). */
export interface Ride {
  flight: HUDFlight;
  count: number;
  id: number;
}

const sameRide = (a: HUDFlight, b: HUDFlight) =>
  a.lane === b.lane &&
  a.from === b.from &&
  a.to === b.to &&
  a.falls === b.falls &&
  a.arrives === b.arrives;

/** One flight in the air at a time. A flight like the one in the air (same
 * lane, same way, same ending) rides along as its count; another waits its
 * turn, and more like it ride along with the one waiting. */
export class FlightQueue {
  private next = 1;
  current: Ride | null = null;
  readonly waiting: Ride[] = [];
  /** Takes a flight; true when what is drawn changed (a start or a count). */
  add(flight: HUDFlight): boolean {
    if (this.current && sameRide(this.current.flight, flight)) {
      this.current = { ...this.current, count: this.current.count + 1 };
      return true;
    }
    const waiting = this.waiting.find((ride) => sameRide(ride.flight, flight));
    if (waiting) {
      waiting.count += 1;
      return false;
    }
    const ride = { flight, count: 1, id: this.next++ };
    if (this.current) {
      this.waiting.push(ride);
      return false;
    }
    this.current = ride;
    return true;
  }
  /** The flight in the air landed: the next waiting one takes off. */
  finish(id: number): boolean {
    if (this.current?.id !== id) return false;
    this.current = this.waiting.shift() ?? null;
    return true;
  }
}
