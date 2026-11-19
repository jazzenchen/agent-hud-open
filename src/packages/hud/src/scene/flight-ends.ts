// Where a flight happens, measured as it starts: beside the sender's and the
// receiver's marks on a dock (the person's bubble for their own words), or
// beside the robots around the notch.
import { botKey } from "../bots/artwork";
import type { HUDRect } from "../geometry";
import type { HUDEdge } from "../model";
import { PERSON, type HUDFlight } from "./flight-queue";
import {
  dockRoute,
  inward,
  notchRoute,
  type NotchStage,
  type Point,
} from "./flight-route";

/** The craft's width and its envelope's height, from the marks' size. */
export const craftSize = (mark: number) => {
  const width = Math.round(Math.min(32, Math.max(18, mark * 0.6)));
  return { width, height: Math.round(width * 0.72) };
};

/** Where a flight happens: the route's ends beside the two marks, the marks
 * (or robots) themselves, and the elements that play the marks' part. */
export interface Ends {
  start: Point;
  end: Point;
  from: Point;
  to: Point;
  route: ReturnType<typeof dockRoute>;
  sender: Element | null;
  receiver: Element | null;
}

/** Around the notch: from beside the sender's robot, under the notch, to beside the receiver's. */
export function notchEnds(stage: NotchStage, flight: HUDFlight) {
  const size = craftSize(34);
  const below = (robot: Point) => ({
    x: robot.x,
    y: stage.top + stage.height + size.height / 2 + 6,
  });
  const [from, to] = flight.fromLeft
    ? [stage.left, stage.right]
    : [stage.right, stage.left];
  const start = below(from);
  const end = below(to);
  return {
    size,
    ends: {
      start,
      end,
      from,
      to,
      route: notchRoute(start, end, stage.dip),
      sender: null,
      receiver: null,
    },
  };
}

/** On a dock: beside the sender's mark and the receiver's, both found where
 * the strip drew them. The person sends from the clipboard's end, or where it
 * sits (`clip`) when this HUD does not show it. */
export function dockEnds(
  root: HTMLElement,
  flight: HUDFlight,
  strip: HUDRect,
  edge: HUDEdge,
  clip?: HUDRect,
) {
  const bounds = root.getBoundingClientRect();
  // The whole mark (its ring included), not the logo inside it.
  const mark = (source: string) => {
    const logo = [
      ...root.querySelectorAll<HTMLElement>(
        `.hud-resting-strip [data-bot-source="${botKey(source)}"]`,
      ),
    ].find((element) => !element.closest("[data-hud-measurement]"));
    return logo?.closest<HTMLElement>(".hud-agent-mark") ?? logo ?? null;
  };
  const person = flight.from === PERSON;
  const sender = person
    ? ([
        ...root.querySelectorAll<HTMLElement>('.hud-dock-end[data-end="clip"]'),
      ].find((element) => !element.closest("[data-hud-measurement]")) ?? null)
    : mark(flight.from);
  const receiver = mark(flight.to);
  // The clipboard's end sits off the strip: the receiver's strip, or the
  // HUD's own when the receiver is still to appear.
  const shelf = (
    [sender, receiver]
      .map((element) => element?.closest(".hud-resting-strip"))
      .find(Boolean) ??
    [...root.querySelectorAll(".hud-resting-strip")].find(
      (element) => !element.closest("[data-hud-measurement]"),
    )
  )?.getBoundingClientRect();
  const box = (element: HTMLElement | null, away = strip) => {
    if (!element)
      return {
        center: {
          x: away.x + away.width / 2,
          y: away.y + away.height / 2,
        },
        side: Math.min(away.width, away.height),
      };
    const rect = element.getBoundingClientRect();
    return {
      center: {
        x: rect.left + rect.width / 2 - bounds.left,
        y: rect.top + rect.height / 2 - bounds.top,
      },
      side: Math.max(rect.width, rect.height),
    };
  };
  const from = box(sender, person && clip ? clip : strip);
  const to = box(receiver);
  const size = craftSize(from.side);
  const n = inward[edge];
  // Just outside the dock's inner edge, level with the mark.
  const inner = (center: Point) => {
    if (!shelf) return 0;
    const left = shelf.left - bounds.left;
    const top = shelf.top - bounds.top;
    if (n.x < 0) return center.x - left;
    if (n.x > 0) return left + shelf.width - center.x;
    if (n.y < 0) return center.y - top;
    return top + shelf.height - center.y;
  };
  const beside = (mark: { center: Point; side: number }) => {
    const reach =
      Math.max(mark.side / 2, inner(mark.center)) + size.width * 0.5 + 6;
    return {
      x: mark.center.x + n.x * reach,
      y: mark.center.y + n.y * reach,
    };
  };
  const start = beside(from);
  const end = beside(to);
  return {
    size,
    ends: {
      start,
      end,
      from: from.center,
      to: to.center,
      route: dockRoute(
        start,
        end,
        edge,
        { width: bounds.width, height: bounds.height },
        size.width,
      ),
      sender,
      receiver,
    },
  };
}
