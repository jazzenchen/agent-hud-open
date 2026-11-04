// BotMotion's body: how it sits for its look and act, the events it plays,
// its fins' flaps and the lift of the part a bot keeps still.
import type { BotMotion } from "./bot-motion";
import { BotMoves, easeOutBack } from "./bot-moves";
import { isAttention, staysInPlace } from "./bot-state";
export interface BotPose {
  x: number;
  y: number;
  rotation: number;
  scaleX: number;
  scaleY: number;
  footing: number;
}
export interface BotFlap {
  left: number;
  right: number;
}
export const botPose = (): BotPose => ({
  x: 0,
  y: 0,
  rotation: 0,
  scaleX: 1,
  scaleY: 1,
  footing: 0,
});
export function mixPose(a: BotPose, b: BotPose, t: number): BotPose {
  return Object.fromEntries(
    Object.keys(a).map((key) => [
      key,
      a[key as keyof BotPose] +
        (b[key as keyof BotPose] - a[key as keyof BotPose]) * t,
    ]),
  ) as unknown as BotPose;
}
export function sitting(
  motion: BotMotion,
  age: number,
  progress: number,
  swell: number,
  dozing: boolean,
) {
  const pose = botPose();
  if (isAttention(motion.look)) {
    if (age < 0.45) {
      const x = age / 0.45,
        arc = Math.sin(Math.PI * x);
      pose.y = -0.26 * 4 * x * (1 - x);
      pose.scaleY = 1 + 0.08 * arc;
      pose.scaleX = 1 - 0.05 * arc;
    }
    const raised = 1 + 0.12 * easeOutBack(Math.min(1, age / 0.35));
    pose.scaleX *= raised;
    pose.scaleY *= raised;
    if (!motion.turnsToCard) {
      const start = BotMoves.move(age).start;
      if (!motion.limbed) {
        if (motion.look === "permission")
          pose.rotation = start === null ? 0 : BotMoves.knock(age - start);
        if (motion.look === "question")
          pose.rotation = BotMoves.questionTilt(age);
        if (motion.look === "plan")
          pose.y += start === null ? 0 : BotMoves.nod(age - start);
      }
    }
  } else if (motion.look === "done") {
    if (age < 0.6) {
      const pop = Math.sin((Math.PI * age) / 0.6);
      pose.scaleX = pose.scaleY = 1 + 0.2 * pop;
      pose.y = -0.1 * pop;
    }
  } else {
    if (motion.breathes(dozing)) {
      const breath = (2 * Math.PI * age) / motion.period;
      pose.scaleX = 1 + 0.03 * Math.sin(2 * breath);
      pose.scaleY = 1 - 0.03 * Math.sin(2 * breath);
    }
    if (motion.look === "thinking" && !motion.limbed)
      pose.rotation =
        BotMoves.ponderTilt *
        Math.max(-1, Math.min(1, motion.face.look / BotMoves.ponderGaze[0]));
  }
  if (motion.act && !motion.limbed) {
    if (motion.act.kind === "tilt")
      pose.rotation += motion.act.side * 9 * swell;
    if (motion.act.kind === "stretch") {
      const arc = Math.sin(Math.PI * progress);
      pose.scaleY *= 1 + 0.06 * arc;
      pose.scaleX *= 1 - 0.04 * arc;
    }
    if (motion.act.kind === "wiggle")
      pose.rotation +=
        8 * Math.sin(2 * Math.PI * 3 * progress) * (1 - progress);
  }
  return pose;
}
export function played(motion: BotMotion, now: number) {
  if (motion.greeting !== null)
    motion.pose.y += BotMoves.hop(now - motion.greeting) * motion.morph;
  if (!motion.event) return;
  const time = now - motion.event.start;
  if (motion.event.kind === "start")
    motion.pose.y += BotMoves.hop(time) * motion.morph;
  else if (motion.event.kind === "failed") {
    const sag = BotMoves.sag(time);
    motion.pose.x += BotMoves.shake(time) * motion.morph;
    motion.pose.rotation += 6 * sag * motion.morph;
    motion.pose.y += 0.02 * sag * motion.morph;
  } else {
    const sigh = BotMoves.sigh(time);
    motion.pose.scaleX *= 1 + (sigh.scaleX - 1) * motion.morph;
    motion.pose.scaleY *= 1 + (sigh.scaleY - 1) * motion.morph;
    motion.pose.footing = 0.5;
  }
}
export function flapping(motion: BotMotion, age: number, now: number): BotFlap {
  let left = 0,
    right = 0;
  if (motion.event) {
    left = right =
      motion.event.kind === "start"
        ? BotMoves.finFlap(now - motion.event.start) * motion.morph
        : 0;
  } else if (motion.greeting !== null) {
    const wave = BotMoves.finWave(now - motion.greeting) * motion.morph;
    if (motion.greetSide < 0 || motion.helper) left = wave;
    else right = wave;
  } else if (motion.look.startsWith("tool-") && !motion.state.holds)
    left = right =
      BotMoves.bob(now - motion.beatsFrom) > 0
        ? BotMoves.paddle * motion.morph
        : 0;
  else if (motion.look === "done")
    left = right = BotMoves.cheer(age) * motion.morph;
  return { left: Math.round(left), right: Math.round(right) };
}
export function keep(motion: BotMotion, age: number, now: number, dt: number) {
  if (staysInPlace(motion.look)) {
    motion.kept = { lift: 0, scale: 1 };
    return;
  }
  let lift = 0,
    scale = 1;
  if (isAttention(motion.look)) {
    lift = 0.03;
    scale = 1.22;
    if (!motion.turnsToCard && (age < BotMoves.fullWait || motion.moving(age)))
      scale += 0.08 * Math.sin((2 * Math.PI * now) / 0.8);
  } else if (motion.look === "done" && age < 0.6)
    lift = 0.22 * Math.sin((Math.PI * age) / 0.6);
  const ease = 1 - Math.exp(-dt * 22);
  motion.kept.lift += (lift - motion.kept.lift) * ease;
  motion.kept.scale += (scale - motion.kept.scale) * ease;
}
