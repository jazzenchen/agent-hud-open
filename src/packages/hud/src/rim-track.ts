// Where a moving rim's silhouette sits, frame by frame, inside the fixed box
// its canvas is drawn on. Pure, so it can be tested apart from the renderer.

/** A rect in the scene's own coordinates. */
export interface RimRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The silhouette the rim wraps, placed inside the rim's box. */
export interface RimShape extends RimRect {
  radius: number;
  /** Concave contact shoulders outside the core; excluded from the rim's ink. */
  flare: number;
}

/**
 * The rim's silhouette as it changes, read once a frame. `subscribe` calls
 * `change` in the frame the silhouette moved, before that frame is drawn.
 */
export interface RimTrack {
  get(): RimShape;
  subscribe(change: () => void): () => void;
}

/** A notch frame without its shoulders: the part the rim draws round. */
export function rimCore(frame: RimRect, flare: number): RimRect {
  return {
    x: frame.x + flare,
    y: frame.y,
    width: Math.max(0, frame.width - flare * 2),
    height: frame.height,
  };
}

/**
 * The box the rim's canvas covers while the notch moves from `from` to `to`:
 * both ends, so every frame between them fits and the canvas keeps its size
 * until the move is over. Resizing a canvas clears it, so a canvas that
 * follows the shape every frame shows a blank or stale rim.
 */
export function rimBox(from: RimRect, to: RimRect): RimRect {
  const x = Math.min(from.x, to.x);
  const y = Math.min(from.y, to.y);
  return {
    x,
    y,
    width: Math.max(from.x + from.width, to.x + to.width) - x,
    height: Math.max(from.y + from.height, to.y + to.height) - y,
  };
}

/** The silhouette inside a box that never moves or changes. */
export function fixedTrack(shape: RimShape): RimTrack {
  return { get: () => shape, subscribe: () => () => {} };
}
