// The scripts ClawdPlayer plays: a step per frame on the 12 fps grid, built
// from a clip's intro, loop and outro or from a shot.
import type { BotSign } from "./bot-state";
import {
  clawdTicks,
  easedOffset,
  zeroOffset,
  type ClawdClip,
  type ClawdFrame,
  type ClawdLook,
  type ClawdOffset,
  type ClawdShot,
} from "./clawd-clips";

export interface Shown {
  look: ClawdLook;
  clip: ClawdClip;
  still: boolean;
  epoch: number | null;
}
export interface Step {
  frame: ClawdFrame | null;
  ticks: number;
  offset: ClawdOffset;
  sign?: BotSign;
  mini?: number;
  holds?: boolean;
  endsCycle?: boolean;
  outro?: boolean;
  mirrored?: boolean;
}
export const frameStep = (
  frame: ClawdFrame | null,
  values: Partial<Step> = {},
): Step => ({ frame, ticks: 1, offset: zeroOffset, ...values });

export function stepsOf(shot: ClawdShot): Step[] {
  return shot.steps.map((step) =>
    frameStep(step.frame, {
      ticks: step.ticks,
      offset: step.offset,
      sign: step.sign,
    }),
  );
}
/** The intro's steps, its offset eased in; the last one ends a cycle. */
export function introSteps(clip: ClawdClip, frames: ClawdFrame[]): Step[] {
  const steps = frames.map((frame, at) =>
    frameStep(frame, {
      ticks: clip.ticks,
      offset: easedOffset(
        clip.offset,
        frames.length > 1 ? at / (frames.length - 1) : 1,
      ),
      sign: clip.sign,
      mini: clip.miniShift,
    }),
  );
  if (steps.length) steps[steps.length - 1].endsCycle = true;
  return steps;
}
/** One turn of the loop, held longer once it has looped as long as its pace
 * says. */
export function cycle(shown: Shown, tick: number): Step[] {
  const clip = shown.clip,
    each = clip.ticks;
  const make = (index: number, ticks: number) =>
    frameStep(
      { name: clip.sprite, index },
      { ticks, offset: clip.offset, sign: clip.sign, mini: clip.miniShift },
    );
  if (shown.still || !clip.loop.length)
    return [
      {
        ...(clip.loop.length
          ? make(clip.loop[0], Infinity)
          : frameStep(null, { ticks: Infinity })),
        holds: true,
      },
    ];
  let steps = clip.loop.map((index) => make(index, each));
  const looped = tick - (shown.epoch ?? tick),
    past = (seconds: number) => looped >= clawdTicks(seconds),
    pace = clip.pace;
  if (pace.kind === "resting" && past(pace.after)) {
    steps[0].ticks += clawdTicks(pace.hold);
    steps[0].holds = true;
  } else if (pace.kind === "waiting" && past(pace.after)) {
    steps[0].ticks = Math.max(
      each,
      clawdTicks(past(pace.slowAfter) ? pace.slowEvery : pace.every) -
        (steps.length - 1) * each,
    );
    steps[0].holds = true;
  } else if (pace.kind === "replaying" && past(pace.after)) {
    steps = [
      {
        ...make(
          pace.hold,
          Math.max(each, clawdTicks(pace.every) - pace.replay.length * each),
        ),
        holds: true,
      },
      ...pace.replay.map((index) => make(index, each)),
    ];
  }
  steps[steps.length - 1].endsCycle = true;
  return steps;
}
/** The outro's steps, its offset eased back out. */
export function outroSteps(clip: ClawdClip): Step[] {
  return clip.outro.map((frame, at) =>
    frameStep(frame, {
      ticks: clip.ticks,
      offset: easedOffset(
        clip.offset,
        clip.outro.length > 1 ? 1 - at / (clip.outro.length - 1) : 0,
      ),
      mini: clip.miniShift,
      outro: true,
    }),
  );
}
/** The loop's frames up to the one a "through" exit leaves by. */
export function exitLead(clip: ClawdClip): Step[] {
  if (clip.exit.kind !== "through") return [];
  const at = clip.loop.indexOf(clip.exit.frame);
  if (at < 0) return [];
  return clip.loop
    .slice(0, at + 1)
    .map((index) =>
      frameStep(
        { name: clip.sprite, index },
        { ticks: clip.ticks, offset: clip.offset, mini: clip.miniShift },
      ),
    );
}
