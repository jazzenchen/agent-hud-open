/** Drops a canvas's drawing buffer when the component drawing on it goes
 * away. WebKit keeps an element that ran a script-made animation (a card's
 * entrance) after it is removed, with its whole subtree, canvases and their
 * buffers included (issue 47); a 0×0 canvas has no buffer. Whoever draws on
 * the canvas next sets its size again. */
export function releaseCanvas(canvas: HTMLCanvasElement | null | undefined) {
  if (canvas) canvas.width = canvas.height = 0;
}
