// The procedural body's parts: box paths, its paint, decoration and ring,
// and a limbed bot's arms and legs.
import type { BotForm } from "./bot-form";
import type { BotSpec, Box, Point, Paint, Decor } from "./bot-spec";
import type { BotLimbs } from "./bot-stance";
export const ink = "#0d0d0f";
export const rectWidth = (b: Box) => b[2] - b[0],
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
export function fillPaint(
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
export function decorate(
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
export function wear(
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
export function ringPath(s: BotSpec, grown: number, near: boolean) {
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
export function drawLimbs(
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
