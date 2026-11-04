// The procedural bot's eyes: drawn eyes that turn, squint, smile and cross,
// and the logo's own eyes on a bot that keeps them.
import type { BotSpec, Box } from "./bot-spec";
import type { BotFace } from "./bot-face";
import { ink, rectWidth, rectHeight } from "./bot-parts";
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
export function eyes(
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
export function ownEyes(
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
