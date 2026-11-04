// BotMotion's face, where its eyes look and when they blink, and the sign
// over its head.
import type { BotMotion } from "./bot-motion";
import { BotMoves } from "./bot-moves";
import {
  badgeSign,
  facingAim,
  isAttention,
  shownSign,
  stillSign,
} from "./bot-state";
export interface BotFace {
  open: number;
  look: number;
  lookY: number;
  wide: number;
  happy: number;
  lean: number;
  crossed: number;
}
export const botFace = (): BotFace => ({
  open: 1,
  look: 0,
  lookY: 0,
  wide: 1,
  happy: 0,
  lean: 0,
  crossed: 0,
});
export function updateFace(
  motion: BotMotion,
  age: number,
  now: number,
  dt: number,
  aim: readonly [number, number],
  swell: number,
  dozing: boolean,
) {
  const target = botFace();
  let every: readonly [number, number] | null = [3.2, 6.5];
  if (isAttention(motion.look)) {
    target.wide = 1.22;
    if (motion.state.facing) {
      const a = facingAim(motion.state.facing);
      target.look = a[0];
      target.lookY = a[1];
    } else if (motion.state.hovered) {
      target.look = aim[0];
      target.lookY = aim[1];
    }
    if (motion.look === "plan" && motion.moving(age)) target.lookY = 0.5;
    every =
      age < 1.5 || age >= BotMoves.fullWait || motion.turnsToCard
        ? null
        : [4, 7];
  } else if (motion.look === "done") {
    if (age < 1.25) {
      target.happy = 1;
      every = null;
    } else {
      target.look = aim[0];
      target.lookY = aim[1];
    }
  } else if (motion.look === "thinking" && !motion.state.hovered) {
    const side = motion.state.holds
      ? motion.heldSide
      : BotMoves.ponderSide(now);
    target.look = BotMoves.ponderGaze[0] * side;
    target.lookY = BotMoves.ponderGaze[1];
    every = [2.2, 4.5];
  } else if (dozing || (motion.look === "compacting" && motion.limbed)) {
    target.open = 0;
    every = null;
  } else {
    target.look = aim[0];
    target.lookY = aim[1];
    every = [2.2, 4.5];
    const act = motion.act;
    if (act) {
      if (act.kind === "glance") {
        target.look += (act.gaze[0] * 0.8 - target.look) * swell;
        target.lookY += (act.gaze[1] - target.lookY) * swell;
      }
      if (act.kind === "squint") target.open = 1 - 0.55 * swell;
      if (act.kind === "widen" || act.kind === "hop")
        target.wide = 1 + 0.2 * swell;
      if (act.kind === "smile") target.happy = swell > 0.4 ? 1 : 0;
      if (act.kind === "inspect") {
        target.lean = act.side * swell;
        target.look += (act.side * 0.6 - target.look) * swell;
        target.lookY -= target.lookY * swell;
      }
      if (act.kind === "stretch" && motion.limbed)
        target.open = 1 - 0.9 * swell;
    }
  }
  if (motion.state.holds) every = null;
  if (motion.pending?.kind === "start") target.open = 0;
  if (motion.event) {
    const time = now - motion.event.start;
    if (motion.event.kind === "start")
      target.open = Math.min(1, time / BotMoves.hopSeconds);
    else if (motion.event.kind === "failed") {
      if (motion.crossesEyes) target.crossed = 1;
      else target.open = 0.45;
      target.lookY = 0.35 * BotMoves.sag(time);
    } else {
      target.open = motion.limbed
        ? time < 4 * BotMoves.beat
          ? 0
          : 0.5
        : BotMoves.sigh(time).open;
      target.lookY = 0.3;
    }
    every = null;
  }
  motion.blinksOn = every !== null;
  const eyes = 1 - Math.exp(-dt * 18),
    head = 1 - Math.exp(-dt * 9);
  motion.face.look += (target.look - motion.face.look) * head;
  motion.face.lookY += (target.lookY - motion.face.lookY) * head;
  for (const key of ["wide", "happy", "lean", "crossed"] as const)
    motion.face[key] += (target[key] - motion.face[key]) * eyes;
  let lid = target.open;
  if (every) {
    if (now >= motion.nextBlink) {
      motion.blinks = [now, -1];
      motion.nextBlink =
        now + every[0] + motion.random.unit() * (every[1] - every[0]);
    }
  } else if (motion.nextBlink < now) motion.nextBlink = now + 1.2;
  for (const start of motion.blinks) {
    const blink = (now - start) / 0.17;
    if (blink >= 0 && blink < 1) lid *= 1 - Math.sin(Math.PI * blink);
  }
  motion.face.open += (lid - motion.face.open) * (1 - Math.exp(-dt * 60));
  motion.blinking =
    Math.abs(lid - motion.face.open) > 0.01 ||
    motion.blinks.some((start) => now >= start && now - start < 0.17);
  motion.following =
    Math.abs(target.look - motion.face.look) > 0.004 ||
    Math.abs(target.lookY - motion.face.lookY) > 0.004 ||
    Math.abs(target.wide - motion.face.wide) > 0.004 ||
    Math.abs(target.happy - motion.face.happy) > 0.01 ||
    Math.abs(target.lean - motion.face.lean) > 0.01 ||
    Math.abs(target.crossed - motion.face.crossed) > 0.01;
}
export function signs(
  motion: BotMotion,
  now: number,
  arrived: boolean,
  dozing: boolean,
) {
  // Arrived, the still mark's sign; on its way, only its badge's.
  let sign = arrived
    ? stillSign(motion.state)
    : motion.state.badge
      ? badgeSign(motion.state.badge)
      : null;
  if (motion.event?.kind === "failed") sign = shownSign(sign, "cross");
  if (motion.morph > 0 && motion.look === "thinking" && !motion.limbed)
    sign = shownSign(sign, "ellipsis");
  if (motion.morph > 0 && dozing) sign = shownSign(sign, "z");
  motion.sign = sign;
  motion.lit =
    sign === "ellipsis" && !motion.state.holds
      ? Math.floor(now * 3 + 1e-6) % 3
      : 0;
}
