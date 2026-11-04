// The procedural bot's body as it grows out of the logo: outline, paint,
// ring, decoration, limbs and eyes, and the helper beside it.
import { formOutline, type BotForm } from "./bot-form";
import type { Point } from "./bot-spec";
import type { BotMotion } from "./bot-motion";
import { botFace } from "./bot-face";
import { botLimbs } from "./bot-stance";
import type { BotFlap } from "./bot-pose";
import { easeInOut } from "./bot-moves";
import { finPath, finSilhouette } from "./bot-fins";
import { BotStill } from "./bot-still";
import { group } from "./scratch";
import { eyes } from "./bot-eyes";
import {
  ink,
  rectWidth,
  rectHeight,
  fillPaint,
  decorate,
  wear,
  ringPath,
  drawLimbs,
} from "./bot-parts";
const cut = (form: BotForm) => (form.light ? "#f5f5f7" : ink);
interface Morph {
  share: number;
  eased: number;
  solid: number;
  dressed: number;
}
export const morph = (share: number): Morph => ({
  share,
  eased: easeInOut(share),
  solid: Math.min(1, share / 0.22),
  dressed: Math.max(0, Math.min(1, (share - 0.45) / 0.45)),
});
function stillPart(
  context: CanvasRenderingContext2D,
  form: BotForm,
  m: Morph,
  turn: number,
  decor: number,
  helper = false,
  flap: BotFlap = { left: 0, right: 0 },
) {
  const s = form.spec,
    [cx, cy] = s.center,
    r = s.radius,
    artwork = 1 - Math.min(1, m.share / 0.4);
  if (artwork > 0) {
    context.globalAlpha = artwork;
    context.drawImage(form.artwork, 0, 0, 1, 1);
  }
  context.translate(0, (s.settle ?? 0) * m.eased);
  const path = formOutline(form, m.eased, turn),
    ringShare = s.ring ? Math.max(0, Math.min(1, (m.share - 0.5) / 0.5)) : 0;
  if (s.ring && ringShare > 0) {
    context.globalAlpha = ringShare;
    context.strokeStyle = form.ink;
    context.lineWidth = s.ring.width * r;
    context.lineCap = "round";
    context.stroke(ringPath(s, ringShare, false));
  }
  for (const item of s.decor ?? [])
    if (m.dressed > 0 && (item.kind === "feelers" || item.kind === "fins")) {
      context.save();
      context.translate(cx, cy);
      context.scale(r, r);
      context.globalAlpha = m.dressed;
      if (item.kind === "feelers") {
        context.strokeStyle = item.color;
        context.lineWidth = item.width;
        context.lineJoin = context.lineCap = "round";
        context.beginPath();
        for (const line of item.lines)
          line.forEach((p, i) =>
            i ? context.lineTo(...p) : context.moveTo(...p),
          );
        context.stroke();
      } else {
        context.fillStyle = item.color;
        const p = new Path2D(),
          grown = 0.3 + 0.7 * m.dressed;
        for (const fin of item.fins)
          p.addPath(
            finPath(
              fin,
              grown,
              fin.hand < 0 ? flap.left : fin.hand > 0 ? flap.right : 0,
            ),
          );
        context.fill(p);
      }
      context.restore();
    }
  context.globalAlpha = m.solid;
  context.save();
  context.clip(path);
  fillPaint(context, form, s.paint);
  if (s.patch) {
    const b = s.patch.box,
      width = rectWidth(b) * r,
      height = rectHeight(b) * r,
      corner = Math.min(s.patch.round * r, Math.min(width, height) / 2),
      px = cx + ((b[0] + b[2]) / 2) * r,
      py = cy + ((b[1] + b[3]) / 2) * r;
    context.save();
    context.translate(px, py);
    context.scale(0.4 + 0.6 * m.eased, 0.4 + 0.6 * m.eased);
    context.globalAlpha =
      m.solid * Math.max(0, Math.min(1, (m.share - 0.2) / 0.4));
    const p = new Path2D();
    p.roundRect(-width / 2, -height / 2, width, height, corner);
    context.clip(p);
    fillPaint(context, form, s.patch.paint, [-px, -py]);
    context.restore();
  }
  if (s.stroke) {
    context.strokeStyle = s.stroke;
    context.lineWidth = 0.07;
    context.lineJoin = "round";
    context.stroke(path);
  }
  context.restore();
  if (s.ring && ringShare > 0) {
    context.save();
    context.clip(path);
    context.globalAlpha = ringShare;
    context.strokeStyle = cut(form);
    context.lineWidth = s.ring.width * r;
    context.lineCap = "butt";
    context.stroke(ringPath(s, ringShare, true));
    context.restore();
  }
  if (!s.keeps && m.dressed > 0) {
    wear(context, s, path, m.dressed);
    for (const item of (s.decor ?? []).slice(0, decor))
      decorate(context, item, s, [0, 0], m.dressed);
  }
  if (helper && m.dressed > 0) drawHelper(context, form, m.dressed);
}
export function drawHelper(
  context: CanvasRenderingContext2D,
  form: BotForm,
  strength: number,
) {
  const s = form.spec,
    scale = 0.4;
  context.save();
  context.translate(s.center[0] + 0.42, s.center[1] + 0.18);
  context.scale(scale, scale);
  context.translate(-s.center[0], -s.center[1]);
  context.globalAlpha = strength;
  if (s.ownEyes) {
    context.save();
    context.translate(0.5, 0.5);
    context.scale(1 + (0.035 * 2) / scale, 1 + (0.035 * 2) / scale);
    context.translate(-0.5, -0.5);
    context.globalCompositeOperation = "destination-out";
    context.drawImage(form.artwork, 0, 0, 1, 1);
    context.restore();
    context.drawImage(form.artwork, 0, 0, 1, 1);
  } else {
    const path = formOutline(form, 1, 0);
    context.save();
    context.globalCompositeOperation = "destination-out";
    context.strokeStyle = "#000";
    context.lineWidth = (0.035 * 2) / scale;
    context.lineJoin = "round";
    const gap = new Path2D();
    gap.addPath(path);
    const fins = finSilhouette(s);
    if (fins) gap.addPath(fins);
    context.stroke(gap);
    context.restore();
    group(context, strength, (c) => {
      c.save();
      stillPart(c, form, morph(1), 0, (s.decor ?? []).length);
      c.restore();
      drawLimbs(c, form, botLimbs(), 1);
      eyes(c, s, botFace(), 0, 0, 1);
    });
  }
  context.restore();
}
export function drawBody(
  context: CanvasRenderingContext2D,
  form: BotForm,
  motion: BotMotion,
  cache: boolean,
) {
  const s = form.spec,
    m = morph(motion.morph),
    decor = s.decor ?? [],
    first = decor.findIndex((item) => item.kind === "blocks" && item.follows),
    settled = s.keeps ? 0 : first < 0 ? decor.length : first,
    still = cache
      ? new BotStill(
          form,
          m.share,
          motion.turn,
          motion.helper,
          motion.flap,
          context,
        )
      : null;
  if (!still?.restore()) {
    context.save();
    stillPart(
      context,
      form,
      m,
      motion.turn,
      settled,
      motion.helper,
      motion.flap,
    );
    context.restore();
    still?.keep();
  }
  context.translate(0, (s.settle ?? 0) * m.eased);
  if (s.keeps) {
    const b = s.keeps,
      cx = (b[0] + b[2]) / 2,
      cy = (b[1] + b[3]) / 2;
    context.save();
    context.globalAlpha = m.solid;
    context.translate(cx, cy - motion.kept.lift);
    context.scale(motion.kept.scale, motion.kept.scale);
    context.translate(-cx, -cy);
    const p = new Path2D();
    p.rect(b[0] - 0.01, b[1] - 0.01, rectWidth(b) + 0.02, rectHeight(b) + 0.02);
    context.clip(p);
    context.drawImage(form.artwork, 0, 0, 1, 1);
    context.restore();
  }
  if (m.dressed <= 0) return;
  drawLimbs(context, form, motion.limbs, m.dressed);
  const face = motion.face,
    turn = s.reach * 0.72,
    yaw = Math.max(-1.15, Math.min(1.15, face.look)) * turn,
    pitch = Math.max(-1.15, Math.min(1.15, face.lookY)) * turn,
    glance: Point = [
      Math.sin(yaw) * s.radius * 0.72,
      Math.sin(pitch) * s.radius * 0.62,
    ];
  if (s.keeps)
    wear(context, s, formOutline(form, m.eased, motion.turn), m.dressed);
  for (const item of decor.slice(settled))
    decorate(
      context,
      item,
      s,
      item.kind === "blocks" && item.follows ? glance : [0, 0],
      m.dressed,
    );
  eyes(context, s, face, yaw, pitch, m.dressed);
}
