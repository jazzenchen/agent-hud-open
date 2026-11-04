// A planned flight played: each of its parts gets its keyframes as a Web
// Animation, all on one clock, and the flight lands once they have run.
import type { RefObject } from "react";
import type { BotCue } from "../bots/bot-state";
import { STORY, stillStory, type Story } from "./flight-plan";
import type { Ends } from "./flight-ends";
import type { HUDFlight } from "./flight-queue";
import type { NotchStage } from "./flight-route";

const keep = { fill: "both", easing: "linear" } as const;

type Part<T extends Element> = RefObject<T | null>;
/** The elements a flight animates. */
export interface FlightParts {
  craft: Part<HTMLDivElement>;
  turn: Part<HTMLDivElement>;
  mirror: Part<HTMLDivElement>;
  paper: Record<"shade" | "edge" | "body", Part<HTMLDivElement>>;
  flap: Part<SVGSVGElement>;
  pocket: Part<SVGSVGElement>;
  folds: Part<SVGSVGElement>;
  sheet: Part<HTMLDivElement>;
  letter: Part<HTMLDivElement>;
  second: Part<HTMLDivElement>;
  trail: Part<HTMLDivElement>;
  wings: Record<"left" | "right", Part<SVGSVGElement>>;
  bots: Record<"left" | "right", Part<HTMLDivElement>>;
}

/** Where a flight happens and its story; no story under reduced motion. */
export interface FlightPlan {
  ends: Ends;
  story: Story | null;
}

/** Hands every part its keyframes and calls `onDone` once the flight has
 * landed; returns what stops it. */
export function playFlight(
  {
    craft,
    turn,
    mirror,
    paper,
    flap,
    pocket,
    folds,
    sheet,
    letter,
    second,
    trail,
    wings,
    bots,
  }: FlightParts,
  plan: FlightPlan,
  {
    flight,
    notch,
    stage,
    onDone,
    setPuzzled,
  }: {
    flight: HUDFlight;
    notch: boolean;
    stage: NotchStage | null;
    onDone: () => void;
    setPuzzled: (cue: BotCue) => void;
  },
): () => void {
  const animations: Animation[] = [];
  const run = (
    element: Element | null | undefined,
    frames: Keyframe[],
    options: KeyframeAnimationOptions,
  ) => {
    if (element)
      animations.push(element.animate(frames, { ...keep, ...options }));
  };
  const { ends } = plan;
  const sender = notch
    ? (flight.fromLeft ? bots.left : bots.right).current
    : ends.sender;
  const receiver = notch
    ? (flight.fromLeft ? bots.right : bots.left).current
    : ends.receiver;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let length: number = STORY.still;
  if (!plan.story) {
    const still = stillStory(flight.falls);
    run(craft.current, still.first, { duration: STORY.still });
    run(second.current, still.second, { duration: STORY.still });
  } else {
    const s = plan.story;
    const delay = notch ? STORY.enter : 0;
    length = notch ? s.length + STORY.leave : s.length;
    run(craft.current, s.move, { duration: s.length });
    run(turn.current, s.turn, { duration: s.length });
    run(mirror.current, s.mirror, { duration: s.length });
    for (const part of Object.values(paper))
      run(part.current, s.shape, { duration: s.length });
    run(craft.current?.firstElementChild, s.pop, { duration: s.length });
    run(flap.current, s.flap, { duration: s.length });
    run(folds.current, s.folds, { duration: s.length });
    run(pocket.current, s.pocket, { duration: s.length });
    run(
      sheet.current,
      s.folds.map((frame) => ({
        offset: frame.offset,
        opacity: 1 - Number(frame.opacity),
      })),
      { duration: s.length },
    );
    run(letter.current, s.letter, { duration: s.length });
    trail.current?.querySelectorAll("[data-dash]").forEach((dash, index) => {
      const frames = s.dashes[index]?.frames;
      if (frames) run(dash, frames, { duration: s.length });
    });
    if (notch && stage) {
      for (const side of ["left", "right"] as const) {
        const receiving = (side === "left") !== flight.fromLeft;
        const appears =
          receiving && flight.arrives ? s.takeoff + STORY.fly * 0.4 : 0;
        const outward = side === "left" ? 1 : -1;
        run(wings[side].current, [{ scale: "0 1" }, { scale: "1 1" }], {
          duration: STORY.enter,
          delay: appears,
          easing: "cubic-bezier(0.2, 0.9, 0.3, 1)",
        });
        run(
          bots[side].current,
          appears
            ? [
                { opacity: 0, scale: "0.3", translate: "0 0" },
                { opacity: 1, scale: "1.18", translate: "0 0", offset: 0.6 },
                { opacity: 1, scale: "1", translate: "0 0" },
              ]
            : [
                {
                  opacity: 0,
                  scale: "1",
                  translate: `${outward * stage.bot}px 0`,
                },
                { opacity: 1, scale: "1", translate: "0 0" },
              ],
          {
            duration: STORY.enter,
            delay: appears,
            easing: "cubic-bezier(0.2, 0.9, 0.3, 1.2)",
          },
        );
        const out = s.length + (side === "left" ? 0 : 40);
        run(wings[side].current, [{ scale: "1 1" }, { scale: "0 1" }], {
          duration: STORY.leave,
          delay: out,
          easing: "ease-in",
          fill: "forwards",
        });
        run(
          bots[side].current,
          [
            { opacity: 1, scale: "1", translate: "0 0" },
            {
              opacity: 0,
              scale: "1",
              translate: `${outward * stage.bot}px 0`,
            },
          ],
          {
            duration: STORY.leave,
            delay: out,
            easing: "ease-in",
            fill: "forwards",
          },
        );
      }
    }
    // The sender hands the letter over with a little push.
    run(
      sender,
      [
        { scale: "1", rotate: "0deg" },
        { scale: "0.9", rotate: "0deg", offset: 0.4 },
        { scale: "1.06", rotate: "0deg", offset: 0.75 },
        { scale: "1", rotate: "0deg" },
      ],
      { duration: 380, delay: delay + 40, easing: "ease-out", fill: "none" },
    );
    if (flight.arrives && !notch)
      run(
        receiver,
        [
          { scale: "1" },
          { scale: "0.55", offset: 0.35 },
          { scale: "1.22", offset: 0.75 },
          { scale: "1" },
        ],
        {
          duration: 420,
          delay: s.takeoff + STORY.fly * 0.3,
          easing: "ease-out",
          fill: "none",
        },
      );
    if (flight.falls) {
      // The sender is left puzzled: a scratch of the head as it falls.
      run(
        sender,
        [
          { rotate: "0deg", scale: "1" },
          { rotate: "-9deg", scale: "1", offset: 0.2 },
          { rotate: "7deg", scale: "1", offset: 0.4 },
          { rotate: "-7deg", scale: "1", offset: 0.6 },
          { rotate: "5deg", scale: "1", offset: 0.8 },
          { rotate: "0deg", scale: "1" },
        ],
        {
          duration: 650,
          delay: s.landing,
          easing: "ease-in-out",
          fill: "none",
        },
      );
      if (notch)
        timer = setTimeout(
          () => setPuzzled({ event: "failed", at: performance.now() / 1000 }),
          s.landing,
        );
    } else {
      // The receiver takes the letter in with a bounce.
      run(
        receiver,
        [
          { scale: "1", translate: "0 0" },
          { scale: "1.18", translate: "0 -2px", offset: 0.4 },
          { scale: "1", translate: "0 0" },
        ],
        {
          duration: 300,
          delay: s.landing + STORY.unfold + STORY.deliver * 0.7,
          easing: "ease-out",
          fill: "none",
        },
      );
    }
  }
  // Landed when every part finished or was cancelled from outside, or at
  // the latest a moment after its planned length: a window whose page is
  // hidden stops its animations' clock, and a ride that never ends would
  // hold its lane for good.
  let ended = false;
  const land = () => {
    if (ended) return;
    ended = true;
    onDone();
  };
  void Promise.allSettled(
    animations.map((animation) => animation.finished),
  ).then(land);
  const fallback = setTimeout(land, length + 600);
  return () => {
    ended = true;
    clearTimeout(fallback);
    if (timer) clearTimeout(timer);
    for (const animation of animations) animation.cancel();
  };
}
