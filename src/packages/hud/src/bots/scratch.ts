/** Canvases a frame draws through and hands back at once, kept for the next
 * frame. A bot animates at up to 24 fps; a new canvas per layer and frame left
 * their bitmaps to the garbage collector, which in WebKit falls far behind. */
const spare: HTMLCanvasElement[] = [];

/** A clear canvas of this size whose context state is saved, for `giveBack`. */
export function borrowCanvas(width: number, height: number) {
  const canvas = spare.pop() ?? document.createElement("canvas");
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  const context = canvas.getContext("2d")!;
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, width, height);
  context.save();
  return { canvas, context };
}

/** Restores the context state `borrowCanvas` saved and keeps the canvas. */
export function giveBack(canvas: HTMLCanvasElement) {
  canvas.getContext("2d")!.restore();
  if (spare.length < 8) spare.push(canvas);
}

/** `canvas` cleared at this size with an identity transform, or a new one. */
export function reuseCanvas(
  canvas: HTMLCanvasElement | undefined,
  width: number,
  height: number,
) {
  if (!canvas) {
    canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    return canvas;
  }
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  } else {
    const context = canvas.getContext("2d")!;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.globalAlpha = 1;
    context.globalCompositeOperation = "source-over";
    context.clearRect(0, 0, width, height);
  }
  return canvas;
}

/** Draws `draw` on a borrowed canvas with `context`'s transform, then lays it
 * over `context` at `alpha` with `operation`. */
export function group(
  context: CanvasRenderingContext2D,
  alpha: number,
  draw: (c: CanvasRenderingContext2D) => void,
  operation: GlobalCompositeOperation = "source-over",
) {
  const { canvas, context: c } = borrowCanvas(
    context.canvas.width,
    context.canvas.height,
  );
  c.setTransform(context.getTransform());
  draw(c);
  context.save();
  context.resetTransform();
  context.globalAlpha = alpha;
  context.globalCompositeOperation = operation;
  context.drawImage(canvas, 0, 0);
  context.restore();
  giveBack(canvas);
}
