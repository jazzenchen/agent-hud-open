import { borrowCanvas, giveBack } from "./scratch";

/** Per size: the mask and the two alpha passes, reused frame after frame. */
const buffers = new Map<
  string,
  { mask: ImageData; rows: Uint8Array; square: Uint8Array }
>();

/** BotHairline's hardened square max-filter, composited behind the frame at half strength. */
export function layHairline(
  context: CanvasRenderingContext2D,
  reach: number,
  light: boolean,
) {
  if (reach <= 0) return;
  reach = Math.floor(reach);
  const width = context.canvas.width,
    height = context.canvas.height,
    own = context.getImageData(0, 0, width, height).data;
  const size = `${width}x${height}`;
  let buffer = buffers.get(size);
  if (!buffer) {
    if (buffers.size >= 8) buffers.clear();
    buffer = {
      mask: context.createImageData(width, height),
      rows: new Uint8Array(width * height),
      square: new Uint8Array(width * height),
    };
    buffers.set(size, buffer);
  }
  const { mask, rows, square } = buffer;
  // A square window's maximum is the maximum, down the column, of each
  // row's maximum, edges clipped alike: the same mask as the 2-D window.
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      let alpha = 0;
      for (let dx = Math.max(-reach, -x); dx <= reach && x + dx < width; dx++)
        alpha = Math.max(alpha, own[(y * width + x + dx) * 4 + 3]);
      rows[y * width + x] = alpha;
    }
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      let alpha = 0;
      for (let dy = Math.max(-reach, -y); dy <= reach && y + dy < height; dy++)
        alpha = Math.max(alpha, rows[(y + dy) * width + x]);
      square[y * width + x] = alpha;
    }
  const shade = light ? 255 : 0,
    data = mask.data;
  for (let i = 0; i < width * height; i++) {
    const at = i * 4;
    data[at] = data[at + 1] = data[at + 2] = shade;
    data[at + 3] = Math.round(Math.min(255, square[i] * 3) * 0.5);
  }
  const { canvas } = borrowCanvas(width, height);
  try {
    canvas.getContext("2d")!.putImageData(mask, 0, 0);
    context.save();
    context.resetTransform();
    context.globalAlpha = 1;
    context.globalCompositeOperation = "destination-over";
    context.drawImage(canvas, 0, 0);
    context.restore();
  } finally {
    giveBack(canvas);
  }
}
