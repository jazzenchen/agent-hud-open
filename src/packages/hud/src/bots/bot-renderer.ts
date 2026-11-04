import type { BotForm } from "./bot-form";
import type { Box } from "./bot-spec";
import type { BotMotion } from "./bot-motion";
import type { BotSign } from "./bot-state";
import { paintSign, signBox } from "./bot-signs";
import { layHairline } from "./bot-hairline";
import { group, reuseCanvas } from "./scratch";
import { ownEyes } from "./bot-eyes";
import { morph, drawHelper, drawBody } from "./bot-body";
const box: Box = [0, 0, 1, 1];
export function botRenderLook(motion: BotMotion) {
  const f = motion.face,
    s = (v: number, steps: number) => Math.round(v * steps);
  return JSON.stringify([
    s(motion.morph, 60),
    s(motion.turn, 0.5),
    s(f.open, 40),
    s(f.look, 50),
    s(f.lookY, 50),
    s(f.wide, 50),
    s(f.happy, 30),
    s(f.lean, 30),
    s(f.crossed, 30),
    s(motion.kept.lift, 300),
    s(motion.kept.scale, 60),
    motion.limbs,
    motion.flap,
    motion.sign,
    motion.lit,
    motion.helper,
  ]);
}
function bare(
  context: CanvasRenderingContext2D,
  form: BotForm,
  motion: BotMotion,
  cache = false,
) {
  if (form.spec.ownEyes) {
    context.globalAlpha = 1 - motion.morph;
    context.drawImage(form.artwork, ...box);
    if (motion.morph > 0) {
      group(
        context,
        motion.morph,
        (c) => {
          c.drawImage(form.stripped ?? form.artwork, ...box);
          ownEyes(c, form.spec.ownEyes!, motion.face, form.ink);
        },
        "lighter",
      );
      if (motion.helper) drawHelper(context, form, motion.morph);
    }
  } else drawBody(context, form, motion, cache);
}
function canvasFor(side: number, outline: number, into?: HTMLCanvasElement) {
  const inner = Math.ceil(side * 1.6),
    canvas = reuseCanvas(into, inner + outline * 2, inner + outline * 2);
  const context = canvas.getContext("2d")!,
    unit = inner / 1.6;
  context.translate(outline + 0.3 * unit, outline + 0.3 * unit);
  context.scale(unit, unit);
  context.imageSmoothingEnabled = true;
  return { canvas, context };
}
/** Draws into `into` when given: a mark redraws the same canvas each frame. */
export function renderBot(
  form: BotForm,
  motion: BotMotion,
  side: number,
  outline = 0,
  into?: HTMLCanvasElement,
) {
  const { canvas, context } = canvasFor(side, outline, into);
  context.save();
  bare(context, form, motion, true);
  context.restore();
  context.resetTransform();
  if (motion.sign)
    paintSign(
      context,
      motion.sign,
      signBox(form, side, outline),
      form.light,
      motion.lit,
      motion.sign === "ellipsis" || motion.sign === "z"
        ? morph(motion.morph).dressed
        : 1,
    );
  if (outline > 0) layHairline(context, outline, form.light);
  return canvas;
}
export function renderLogo(
  form: BotForm,
  sign: BotSign | null,
  side: number,
  outline = 0,
) {
  const { canvas, context } = canvasFor(side, outline);
  context.drawImage(form.artwork, 0, 0, 1, 1);
  context.resetTransform();
  if (sign) paintSign(context, sign, signBox(form, side, outline), form.light);
  if (outline > 0) layHairline(context, outline, form.light);
  return canvas;
}
