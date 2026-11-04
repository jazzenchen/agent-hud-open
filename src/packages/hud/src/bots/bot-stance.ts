// A limbed bot's arms and legs, in pixel steps, for its look, act or event.
import type { BotMotion } from "./bot-motion";
import { BotMoves } from "./bot-moves";
import { isAttention, staysInPlace, type BotEvent } from "./bot-state";
export interface BotLimbs {
  left: number;
  right: number;
  lifted: number;
  x: number;
  y: number;
  spinner: number;
}
export const botLimbs = (): BotLimbs => ({
  left: 0,
  right: 0,
  lifted: 0,
  x: 0,
  y: 0,
  spinner: 0,
});
const hopPath = [
  [1, 3, 0],
  [-2, 0, 1],
  [-3, 0, 2],
  [-3, 0, 2],
  [-2, 0, 1],
  [0, 0, 0],
  [1, 3, 0],
  [0, 0, 0],
] as const;
export function stance(
  motion: BotMotion,
  age: number,
  now: number,
  dozing: boolean,
) {
  if (motion.event)
    return eventStance(motion.event.kind, now - motion.event.start);
  const limbs = botLimbs(),
    beat = (seconds: number, every: number) =>
      Math.floor(Math.max(0, seconds) / every + 1e-6),
    clock = now - motion.beatsFrom;
  if (
    motion.look === "thinking" ||
    motion.look.startsWith("tool-") ||
    motion.look === "plain"
  ) {
    if (!motion.sign) {
      const frame = motion.state.holds ? 0 : beat(now, BotMoves.beat) % 10;
      limbs.spinner = frame < 6 ? frame + 1 : 11 - frame;
    }
    if (
      motion.look !== "thinking" &&
      !motion.state.holds &&
      BotMoves.inBurst(clock)
    ) {
      const tap = beat(clock, BotMoves.beat);
      limbs.left = tap % 2;
      limbs.right = 1 - limbs.left;
      limbs.y = Math.floor(tap / 2) % 2;
    }
  } else if (motion.look === "compacting") {
    limbs.left = limbs.right = 1;
    limbs.lifted = 3;
  } else if (motion.turnsToCard) {
    if (motion.state.facing === "left") limbs.left = 2;
    else if (motion.state.facing === "right") limbs.right = 2;
  } else if (isAttention(motion.look)) {
    const move = BotMoves.move(age),
      into = move.start === null ? null : age - move.start;
    if (age < BotMoves.firstMove || motion.moving(age)) {
      limbs.left = 1 + (beat(clock, BotMoves.beat) % 2);
      limbs.right = 3 - limbs.left;
      if (into !== null && beat(into, BotMoves.beat) % 2 === 0) {
        limbs.y = -2;
        limbs.lifted = 3;
      }
    } else limbs.right = 2;
  } else if (motion.look === "done" && age < 1.5) {
    const step = beat(clock, 2 * BotMoves.beat);
    limbs.left = step % 2 === 0 ? 2 : 0;
    limbs.right = 2 - limbs.left;
    if (step % 2 === 1) {
      limbs.y = -2;
      limbs.lifted = 3;
    }
  } else if (motion.look === "asleep" && dozing) {
    limbs.lifted = 3;
    limbs.y = 1;
  }
  const act = motion.act;
  if (!act || !staysInPlace(motion.look)) return limbs;
  const since = now - act.start,
    ending = since > act.length - 0.15,
    frame = (count: number) =>
      Math.min(count - 1, beat(since, act.length / count));
  if (["wave", "scuttle", "hop", "stretch"].includes(act.kind))
    limbs.left = limbs.right = limbs.y = 0;
  if (act.kind === "wave") {
    if (since < 0.24) limbs.y = -1;
    const arm = since < 0.12 || ending ? 1 : 2 - (beat(since - 0.12, 0.16) % 2);
    if (act.side < 0) limbs.left = arm;
    else limbs.right = arm;
  }
  if (act.kind === "scuttle") {
    const path = [0, 1, 2, 1, 0, -1, -2, -1, 0],
      step = frame(path.length);
    limbs.x = path[step] * (act.side < 0 ? -1 : 1);
    limbs.lifted = step === 0 || step === path.length - 1 ? 0 : 1 + (step % 2);
  }
  if (act.kind === "hop") {
    const at = hopPath[frame(hopPath.length)];
    limbs.y = at[0];
    limbs.lifted = at[1];
    limbs.left = limbs.right = at[2];
  }
  if (act.kind === "stretch" && since > 0.15 && !ending) {
    limbs.left = limbs.right = 2;
    limbs.y = -1;
  }
  return limbs;
}
export function eventStance(event: BotEvent, time: number) {
  const limbs = botLimbs(),
    beat = (every: number) => Math.floor(Math.max(0, time) / every + 1e-6);
  if (event === "start") {
    const at = hopPath[Math.min(hopPath.length - 1, beat(BotMoves.beat))];
    limbs.y = at[0];
    limbs.lifted = at[1];
    limbs.left = limbs.right = at[2];
  } else if (event === "failed") {
    if (time < 0.5) limbs.x = beat(1 / 12) % 2 === 0 ? 1 : -1;
    else {
      limbs.y = 1;
      limbs.lifted = 3;
    }
  } else if (time < 4 * BotMoves.beat) {
    limbs.left = limbs.right = 2;
    limbs.y = -1;
  } else {
    limbs.y = 1;
    limbs.lifted = 3;
  }
  return limbs;
}
