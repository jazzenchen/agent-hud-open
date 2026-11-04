import type { BotSign } from "./bot-state";
import type { BotForm } from "./bot-form";
export interface SignRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export const SIGN_ROWS: Record<BotSign, readonly string[]> = {
  exclamation: ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#.."],
  question: [".###.", "#...#", "....#", "..##.", "..#..", ".....", "..#.."],
  lines: [".....", "#####", ".....", "#####", ".....", "#####", "....."],
  cross: [".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "....."],
  hourglass: [".....", "#####", ".#.#.", "..#..", ".###.", "#####", "....."],
  ellipsis: [".....", ".....", ".....", ".....", ".....", ".....", "#.#.#"],
  z: [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####"],
};
export const signCell = (side: number) =>
  Math.max(1, Math.min(Math.round(side / 20), Math.round((0.3 * side) / 7)));
export function paintSign(
  context: CanvasRenderingContext2D,
  sign: BotSign,
  rect: SignRect,
  light: boolean,
  lit = 0,
  alpha = 1,
) {
  const matrix = context.getTransform(),
    corners = [
      [rect.x, rect.y],
      [rect.x + rect.width, rect.y + rect.height],
    ].map(([x, y]) => ({
      x: matrix.a * x + matrix.c * y + matrix.e,
      y: matrix.b * x + matrix.d * y + matrix.f,
    })),
    left = Math.round(Math.min(...corners.map((p) => p.x))),
    top = Math.round(Math.min(...corners.map((p) => p.y))),
    cell = Math.max(1, Math.round(Math.abs(corners[1].x - corners[0].x) / 5)),
    bright: SignRect[] = [],
    dim: SignRect[] = [];
  lit = Math.max(0, Math.min(2, lit));
  SIGN_ROWS[sign].forEach((row, y) =>
    [...row].forEach((value, x) => {
      if (value === "#")
        (sign === "ellipsis" && x !== 2 * lit ? dim : bright).push({
          x: left + x * cell,
          y: top + y * cell,
          width: cell,
          height: cell,
        });
    }),
  );
  const boxes = (list: SignRect[], grow = 0) => {
    const path = new Path2D();
    for (const r of list)
      path.rect(
        r.x - grow,
        r.y - grow,
        r.width + 2 * grow,
        r.height + 2 * grow,
      );
    return path;
  };
  context.save();
  context.resetTransform();
  context.globalCompositeOperation = "source-over";
  context.globalAlpha *= alpha;
  if (!light) {
    const ring = boxes(dim, 1);
    ring.addPath(boxes(dim));
    context.fillStyle = "rgba(13,13,15,.21)";
    context.fill(ring, "evenodd");
    context.fillStyle = "rgba(13,13,15,.6)";
    context.fill(boxes(bright, 1));
  }
  context.fillStyle = light ? "#0d0d0f" : "#ffffff";
  context.fill(boxes(bright));
  context.globalAlpha *= 0.35;
  context.fill(boxes(dim));
  context.restore();
}
function placed(
  left: number,
  bottom: number,
  ceiling: number,
  cell: number,
  right: number,
): SignRect {
  const width = 5 * cell,
    height = 7 * cell;
  bottom = Math.max(Math.round(bottom), Math.ceil(ceiling) + height);
  left = Math.max(1, Math.min(Math.round(left), right - 1 - width));
  return { x: left, y: bottom - height, width, height };
}
export function signBox(form: BotForm, side: number, outline = 0): SignRect {
  const inner = Math.ceil(side * 1.6),
    unit = inner / 1.6,
    pixels = (value: number) => outline + (value + 0.3) * unit,
    foot = form.spec.signAt ?? [form.spec.center[0] + 0.14, form.top - 0.04];
  return placed(
    pixels(foot[0]),
    pixels(foot[1]),
    pixels(-0.28),
    signCell(side),
    inner + outline * 2,
  );
}
export function stillSignImage(
  sign: BotSign,
  form: BotForm | null,
  side: number,
  scale: number,
  light: boolean,
) {
  scale = Math.max(1, scale);
  const pixels = side * scale,
    foot =
      form?.spec.signAt ??
      (form ? [form.spec.center[0] + 0.14, form.logoTop - 0.04] : [0.64, 0.06]),
    box = placed(
      foot[0] * pixels,
      foot[1] * pixels,
      -0.28 * pixels,
      signCell(pixels),
      1.3 * pixels,
    ),
    canvas = document.createElement("canvas");
  canvas.width = box.width + 2;
  canvas.height = box.height + 2;
  paintSign(
    canvas.getContext("2d")!,
    sign,
    { x: 1, y: 1, width: box.width, height: box.height },
    light,
  );
  return {
    image: canvas,
    frame: {
      x: (box.x - 1) / scale,
      y: (box.y - 1) / scale,
      width: canvas.width / scale,
      height: canvas.height / scale,
    },
  };
}
