import { formOutline, type BotForm } from "./bot-form";
import type { BotSpec, Box, Point, Paint, Decor } from "./bot-spec";
import {
  BotMotion,
  botFace,
  botLimbs,
  type BotFace,
  type BotLimbs,
  type BotFlap,
  type BotPose,
} from "./bot-motion";
import { easeInOut } from "./bot-moves";
import type { BotSign } from "./bot-state";
import { finPath, finSilhouette } from "./bot-fins";
import { paintSign, signBox } from "./bot-signs";
import { layHairline } from "./bot-hairline";
import { BotStill } from "./bot-still";
import { borrowCanvas, giveBack, reuseCanvas } from "./scratch";
const margin = 0.3,
  ink = "#0d0d0f",
  box: Box = [0, 0, 1, 1];
const cut = (form: BotForm) => (form.light ? "#f5f5f7" : ink);
const rectWidth = (b: Box) => b[2] - b[0],
  rectHeight = (b: Box) => b[3] - b[1];
function roundBox(path: Path2D, b: Box, round: number) {
  path.roundRect(
    b[0],
    b[1],
    rectWidth(b),
    rectHeight(b),
    (Math.min(rectWidth(b), rectHeight(b)) / 2) * round,
  );
}
function boxesPath(boxes: readonly Box[], round = 0) {
  const path = new Path2D();
  for (const box of boxes) roundBox(path, box, round);
  return path;
}
function group(
  context: CanvasRenderingContext2D,
  alpha: number,
  draw: (c: CanvasRenderingContext2D) => void,
  operation: GlobalCompositeOperation = "source-over",
) {
  const { canvas, context: c } = borrowCanvas(
    context.canvas.width,
    context.canvas.height,
  );
  try {
    c.setTransform(context.getTransform());
    draw(c);
    context.save();
    context.resetTransform();
    context.globalAlpha = alpha;
    context.globalCompositeOperation = operation;
    context.drawImage(canvas, 0, 0);
    context.restore();
  } finally {
    giveBack(canvas);
  }
}
export function applyBotPose(
  context: CanvasRenderingContext2D,
  pose: BotPose,
  side = 1,
) {
  const foot = pose.footing * side;
  context.translate(pose.x * side, pose.y * side + foot);
  context.rotate((pose.rotation * Math.PI) / 180);
  context.scale(pose.scaleX, pose.scaleY);
  context.translate(0, -foot);
}
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
function fillPaint(
  context: CanvasRenderingContext2D,
  form: BotForm,
  paint?: Paint,
  origin: Point = [0, 0],
) {
  const s = form.spec,
    at = (point: Point): Point => [
      origin[0] + s.center[0] + (point[0] - 0.5) * 2 * s.radius,
      origin[1] + s.center[1] + (point[1] - 0.5) * 2 * s.radius,
    ];
  if (!paint) {
    if (form.field)
      context.drawImage(
        form.field,
        origin[0] - 0.25,
        origin[1] - 0.25,
        1.5,
        1.5,
      );
    else {
      context.fillStyle = form.ink;
      context.fillRect(origin[0] - 0.3, origin[1] - 0.3, 1.6, 1.6);
    }
    return;
  }
  if (typeof paint === "string") context.fillStyle = paint;
  else {
    const gradient =
      "center" in paint
        ? context.createRadialGradient(
            ...at(paint.center),
            paint.inner * 2 * s.radius,
            ...at(paint.center),
            paint.outer * 2 * s.radius,
          )
        : context.createLinearGradient(...at(paint.from), ...at(paint.to));
    paint.colors.forEach((color, i) =>
      gradient.addColorStop(i / (paint.colors.length - 1), color),
    );
    context.fillStyle = gradient;
  }
  context.fillRect(origin[0] - 0.3, origin[1] - 0.3, 1.6, 1.6);
}
function decorate(
  context: CanvasRenderingContext2D,
  item: Decor,
  s: BotSpec,
  offset: Point,
  strength: number,
) {
  context.save();
  context.translate(s.center[0] + offset[0], s.center[1] + offset[1]);
  context.scale(s.radius, s.radius);
  context.globalAlpha = strength;
  if (item.kind === "blocks") {
    context.fillStyle = item.color;
    context.fill(boxesPath(item.boxes));
  } else if (item.kind === "cut") {
    context.globalCompositeOperation = "destination-out";
    context.fillStyle = "#000";
    context.fill(boxesPath(item.boxes));
  } else if (item.kind === "notch" || item.kind === "line") {
    const p = new Path2D();
    item.points.forEach((point, i) =>
      i ? p.lineTo(...point) : p.moveTo(...point),
    );
    if (item.kind === "notch") {
      p.closePath();
      context.globalCompositeOperation = "destination-out";
      context.fillStyle = "#000";
      context.fill(p);
    } else {
      context.strokeStyle = ink;
      context.lineWidth = item.width;
      context.lineJoin = context.lineCap = "round";
      context.stroke(p);
    }
  }
  context.restore();
}
function wear(
  context: CanvasRenderingContext2D,
  s: BotSpec,
  outline: Path2D,
  strength: number,
) {
  for (const item of s.decor ?? [])
    if (item.kind === "soft") {
      context.save();
      context.clip(outline);
      context.translate(...s.center);
      context.scale(s.radius, s.radius);
      context.globalAlpha = strength;
      context.fillStyle = item.color;
      context.fill(boxesPath(item.boxes, item.round ?? 0));
      context.restore();
    }
}
function ringPath(s: BotSpec, grown: number, near: boolean) {
  const ring = s.ring!,
    p = new Path2D(),
    r = s.radius,
    size = 0.5 + 0.5 * grown,
    tilt = (ring.tilt * Math.PI) / 180,
    sweep = near ? Math.PI : 2 * Math.PI;
  for (let step = 0; step <= 48; step++) {
    const a = (sweep * step) / 48,
      x = ring.radiusX * r * size * Math.cos(a),
      y = ring.radiusY * r * size * Math.sin(a),
      point: Point = [
        s.center[0] + x * Math.cos(tilt) - y * Math.sin(tilt),
        s.center[1] + ring.drop * r + x * Math.sin(tilt) + y * Math.cos(tilt),
      ];
    if (step) p.lineTo(...point);
    else p.moveTo(...point);
  }
  if (!near) p.closePath();
  return p;
}
function spinner(path: Path2D, frame: number, center: Point, size: number) {
  const [spokes, length, width] = [
    [0, 0.24, 0.24],
    [4, 0.55, 0.2],
    [4, 0.85, 0.22],
    [6, 0.9, 0.2],
    [8, 1, 0.2],
    [8, 1, 0.28],
  ][frame - 1];
  if (!spokes) {
    path.rect(
      center[0] - (length * size) / 2,
      center[1] - (length * size) / 2,
      length * size,
      length * size,
    );
    return;
  }
  for (let i = 0; i < spokes; i++) {
    const p = new Path2D(),
      w = width * size;
    p.roundRect(-w / 2, -w / 2, length * size + w, w, w / 2);
    path.addPath(
      p,
      new DOMMatrix().translate(...center).rotate(-90 + (i * 360) / spokes),
    );
  }
}
function drawLimbs(
  context: CanvasRenderingContext2D,
  form: BotForm,
  stance: BotLimbs,
  strength: number,
) {
  const s = form.spec,
    limbs = s.limbs;
  if (!limbs) return;
  const pieces: Box[] = [],
    step = limbs.pixel;
  for (const arm of limbs.arms) {
    const out = (arm[0] + arm[2]) / 2 < 0 ? -1 : 1,
      raised = out < 0 ? stance.left : stance.right,
      y = raised > 0 ? -step : 0;
    pieces.push([arm[0], arm[1] + y, arm[2], arm[3] + y]);
    if (raised > 1)
      pieces.push([
        arm[0] + out * rectWidth(arm),
        arm[1] - 2 * step,
        arm[2] + out * rectWidth(arm),
        arm[3] - 2 * step,
      ]);
  }
  limbs.legs.forEach((leg, i) => {
    const lifted =
        stance.lifted === 3 ||
        (stance.lifted > 0 && i % 2 === stance.lifted - 1),
      y = lifted ? -step / 2 : 0;
    pieces.push([leg[0], leg[1] + y, leg[2], leg[3] + y]);
  });
  const local = boxesPath(pieces);
  if (stance.spinner > 0)
    spinner(local, stance.spinner, [2.2 * step, -1 - 1.5 * step], 0.75 * step);
  const p = new Path2D();
  p.addPath(local, new DOMMatrix().translate(...s.center).scale(s.radius));
  context.save();
  context.clip(p);
  context.globalAlpha = strength;
  fillPaint(context, form, s.paint);
  context.restore();
}
function smile(
  context: CanvasRenderingContext2D,
  half: number,
  stroke: number,
  paint: string,
  strength: number,
) {
  const rise = half * 0.95;
  context.globalAlpha = strength;
  context.strokeStyle = paint;
  context.lineWidth = stroke;
  context.lineCap = "round";
  context.beginPath();
  context.moveTo(-half, rise * 0.45);
  context.quadraticCurveTo(0, -rise * 1.1, half, rise * 0.45);
  context.stroke();
}
function pill(width: number, height: number, round: number) {
  const p = new Path2D(),
    corner = (Math.min(width, height) / 2) * round;
  if (corner > 0.0005)
    p.roundRect(-width / 2, -height / 2, width, height, corner);
  else p.rect(-width / 2, -height / 2, width, height);
  return p;
}
function eyes(
  context: CanvasRenderingContext2D,
  s: BotSpec,
  face: BotFace,
  yaw: number,
  pitch: number,
  awake: number,
) {
  const r = s.radius,
    size = 0.78,
    spread = s.eye.spread,
    flat = s.eye.across,
    grain = s.limbs ? s.limbs.pixel / 2 : null,
    snapped = (v: number) => (grain ? Math.round(v / grain) * grain : v),
    paint = s.eyeColor ?? ink,
    apart =
      flat !== undefined
        ? 2 * flat * r
        : (Math.sin(yaw + spread) - Math.sin(yaw - spread)) * r * 0.76;
  for (const side of [-1, 1]) {
    const angle = flat === undefined ? yaw + side * spread : 0,
      x =
        s.center[0] +
        s.face[0] * r +
        (flat !== undefined
          ? side * flat * r + snapped(Math.sin(yaw) * 0.76) * r
          : Math.sin(angle) * r * 0.76),
      y =
        s.center[1] +
        s.face[1] * r +
        (flat === undefined
          ? Math.sin(pitch) * 0.62
          : snapped(Math.sin(pitch) * 0.62)) *
          r,
      narrow = Math.max(0.25, Math.cos(angle)) ** 1.4,
      tall = (0.72 + 0.28 * Math.cos(angle)) * Math.sqrt(Math.cos(pitch)),
      big =
        (1 + 0.26 * side * face.lean) * face.wide * (0.6 + 0.4 * awake) * size,
      width = s.eye.width * r * big * narrow,
      height =
        s.eye.height * r * big * tall * Math.max(0.08, face.open) * awake;
    context.save();
    context.translate(x, y);
    if (face.happy < 0.98 && face.crossed < 0.98) {
      context.globalAlpha = (1 - face.happy) * (1 - face.crossed);
      context.fillStyle = paint;
      context.fill(pill(width, Math.max(height, width * 0.16), s.eye.round));
      if (s.pupil && height > width * 0.45) {
        context.fillStyle = s.pupil;
        const across = width * 0.21,
          down = Math.min(height, width) * 0.21;
        context.beginPath();
        context.ellipse(
          face.look * width * 0.2,
          face.lookY * height * 0.2 - height * 0.06,
          across,
          down,
          0,
          0,
          2 * Math.PI,
        );
        context.fill();
      }
    }
    if (face.crossed > 0.02) {
      const reach = s.eye.height * r * size * 0.32 * (0.6 + 0.4 * awake);
      context.globalAlpha = face.crossed;
      context.strokeStyle = paint;
      context.lineWidth = s.eye.width * r * size * 0.55;
      context.lineCap = "round";
      context.beginPath();
      context.moveTo(-reach, -reach);
      context.lineTo(reach, reach);
      context.moveTo(-reach, reach);
      context.lineTo(reach, -reach);
      context.stroke();
    }
    if (face.happy > 0.02) {
      const stroke = Math.min(s.eye.width * r * 0.58 * size, apart * 0.2),
        half =
          Math.max(
            stroke * 0.7,
            Math.min(s.eye.width * r * 0.8 * size, apart / 2 - stroke * 1.05),
          ) * Math.max(0.6, narrow);
      smile(context, half, stroke, paint, face.happy);
    }
    context.restore();
  }
}
function ownEyes(
  context: CanvasRenderingContext2D,
  boxes: Box[],
  face: BotFace,
  ink: string,
) {
  const center = (b: Box) => (b[0] + b[2]) / 2,
    middle = (center(boxes[0]) + center(boxes.at(-1)!)) / 2,
    apart = Math.abs(center(boxes.at(-1)!) - center(boxes[0]));
  for (const b of boxes) {
    const side = center(b) > middle ? 1 : -1,
      big = (1 + 0.26 * side * face.lean) * face.wide,
      width = rectWidth(b) * big,
      height = rectHeight(b) * big * Math.max(0.06, face.open);
    context.save();
    context.translate(
      center(b) + Math.min(0.04, rectWidth(b) * 0.45) * face.look,
      (b[1] + b[3]) / 2 + Math.min(0.03, rectHeight(b) * 0.14) * face.lookY,
    );
    if (face.happy < 0.98) {
      context.globalAlpha = 1 - face.happy;
      context.fillStyle = ink;
      context.fill(pill(width, Math.max(height, width * 0.16), 1));
    }
    if (face.happy > 0.02)
      smile(
        context,
        Math.min(rectWidth(b) * 0.95, apart * 0.3),
        Math.min(rectWidth(b) * 0.6, apart * 0.2),
        ink,
        face.happy,
      );
    context.restore();
  }
}
interface Morph {
  share: number;
  eased: number;
  solid: number;
  dressed: number;
}
const morph = (share: number): Morph => ({
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
function drawHelper(
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
function drawBody(
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
/** Draw in normalized mark coordinates. A cached image stays upright; its host applies pose independently. */
export function drawBot(
  context: CanvasRenderingContext2D,
  form: BotForm,
  motion: BotMotion,
  options: { pose?: boolean; signs?: boolean } = {},
) {
  context.save();
  if (options.pose !== false) {
    context.translate(0.5, 0.5);
    applyBotPose(context, motion.pose);
    context.translate(-0.5, -0.5);
  }
  context.save();
  bare(context, form, motion);
  context.restore();
  if (options.signs !== false && motion.sign) {
    const t = context.getTransform(),
      side = Math.hypot(t.a, t.b),
      place = signBox(form, side),
      unit = Math.ceil(side * 1.6) / 1.6;
    paintSign(
      context,
      motion.sign,
      {
        x: (place.x - 0.3 * unit) / unit,
        y: (place.y - 0.3 * unit) / unit,
        width: place.width / unit,
        height: place.height / unit,
      },
      form.light,
      motion.lit,
      motion.sign === "ellipsis" || motion.sign === "z"
        ? morph(motion.morph).dressed
        : 1,
    );
  }
  context.restore();
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
export const BotRenderer = {
  margin,
  image: renderBot,
  logo: renderLogo,
  look: botRenderLook,
};
