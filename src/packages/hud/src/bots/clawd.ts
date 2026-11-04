// Original Anthropic sprite data; attribution is retained in assets/THIRD_PARTY_NOTICES.txt.
import { clawdClips, swiftRound, type ClawdFrame } from "./clawd-clips";
import type { ClawdSheet, ClawdSprite } from "./clawd-sheet";
import type { BotSign } from "./bot-state";
import { SIGN_ROWS } from "./bot-signs";
import { layHairline } from "./bot-hairline";
import { reuseCanvas } from "./scratch";
export type MarkState = "idle" | "working" | "asking" | "done";
export interface SpritePicture extends ClawdFrame {
  offsetX?: number;
  offsetY?: number;
  sign?: BotSign;
  mini?: number;
  mirrored?: boolean;
}
/** Device pixels to two sprite pixels, for a mark `side` points wide on a
 * display of `scale` pixels a point. Open rounds this down, which leaves
 * Clawd's 24-pixel body as little as seven tenths of the mark: 12 pt in a
 * 17 pt reminder on a Retina screen, a size smaller than the logo it stands
 * in for. This app rounds to the nearest, so the body comes to about the mark's
 * width (18 pt there). An odd grain lays sprite pixels as runs of one and two
 * device pixels, as the Swift app's baker already does. */
export function clawdGrain(side: number, scale: number) {
  return Math.max(1, Math.round((1.8 * side * scale) / 24));
}
/** Whether a mark this size shows the sprites: at least one device pixel to a
 * sprite pixel by Open's rounded-down grain, so no smaller mark than in Open
 * swaps its logo for a Clawd larger than itself. */
export function drawsClawdSprites(side: number, scale: number) {
  return Math.floor((1.8 * side * scale) / 24 + 0.05 + 1e-9) >= 2;
}
function fillSprite(
  context: CanvasRenderingContext2D,
  sprite: ClawdSprite,
  index: number,
  footX: number,
  footY: number,
  grain: number,
  per: number,
  mirrored: boolean,
) {
  const frame = sprite.frames[index];
  for (let y = 0; y < sprite.height; y++)
    for (let x = 0; x < sprite.width; x++) {
      const color = frame[y * sprite.width + x];
      if (color === 255) continue;
      const at = mirrored ? sprite.anchorX - x - 1 : x - sprite.anchorX;
      const left = footX + Math.floor((at * grain) / per),
        right = footX + Math.floor(((at + 1) * grain) / per);
      const top = footY + Math.floor(((y - sprite.anchorY) * grain) / per),
        bottom = footY + Math.floor(((y + 1 - sprite.anchorY) * grain) / per);
      context.fillStyle = sprite.palette[color];
      context.fillRect(left, top, right - left, bottom - top);
    }
}
function drawClawdPixels(
  context: CanvasRenderingContext2D,
  sheet: ClawdSheet,
  picture: SpritePicture,
  side: number,
  scale: number,
  extent: number,
  light: boolean,
) {
  const sprite = sheet[picture.name];
  const grain = clawdGrain(side, scale),
    flip = picture.mirrored ? -1 : 1;
  const footX =
    Math.floor(extent / 2) +
    swiftRound(((picture.offsetX ?? 0) * flip * grain) / 2);
  const footY =
    swiftRound(extent / 2 + side * scale * 0.375) +
    swiftRound(((picture.offsetY ?? 0) * grain) / 2);
  context.imageSmoothingEnabled = false;
  fillSprite(
    context,
    sprite,
    picture.index,
    footX,
    footY,
    grain,
    2,
    picture.mirrored ?? false,
  );
  if (picture.mini !== undefined && sheet.Walking)
    fillSprite(
      context,
      sheet.Walking,
      0,
      Math.floor(extent / 2) + swiftRound((picture.mini * grain) / 2),
      swiftRound(extent / 2 + side * scale * 0.375),
      grain,
      4,
      false,
    );
  if (picture.sign) {
    const cell = Math.max(
      1,
      Math.min(
        swiftRound((side * scale) / 20),
        swiftRound((0.3 * side * scale) / 7),
      ),
    );
    const shift = clawdClips.signShifts[picture.name] ?? { x: 0, y: 0 };
    const left =
      footX +
      swiftRound((shift.x * grain) / 2) +
      swiftRound(0.14 * side * scale);
    const top = Math.max(
      footY +
        swiftRound((shift.y * grain) / 2) -
        grain * 8 -
        swiftRound(0.04 * side * scale) -
        cell * 7,
      Math.ceil(extent / 2 - side * scale * 0.78),
    );
    drawSign(context, left, top, cell, light, picture.sign);
  }
}

/** ClawdBaker's bounded 64-frame LRU. No bitmap is scaled after its device-pixel placement. */
export class ClawdBaker {
  private readonly kept = new Map<string, HTMLCanvasElement>();
  constructor(readonly sheet: ClawdSheet) {}
  get count() {
    return this.kept.size;
  }
  purge() {
    this.kept.clear();
  }
  image(
    picture: SpritePicture,
    side: number,
    scale: number,
    extent: number,
    light: boolean,
    outline = Math.max(
      0,
      Math.floor((extent - Math.ceil(side * scale * 1.6)) / 2),
    ),
  ) {
    if (!this.sheet[picture.name]?.frames[picture.index]) return null;
    const key = JSON.stringify([picture, side, scale, extent, light, outline]);
    let canvas = this.kept.get(key);
    if (canvas) this.kept.delete(key);
    else {
      // A full cache bakes into the canvas of the frame it lets go.
      let oldest: HTMLCanvasElement | undefined;
      if (this.kept.size >= 64) {
        const [first, image] = this.kept.entries().next().value!;
        this.kept.delete(first);
        oldest = image;
      }
      canvas = reuseCanvas(oldest, extent, extent);
      const context = canvas.getContext("2d");
      if (!context) return null;
      drawClawdPixels(context, this.sheet, picture, side, scale, extent, light);
      if (outline > 0) layHairline(context, outline, light);
    }
    this.kept.set(key, canvas);
    return canvas;
  }
}
export function drawSign(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  cell: number,
  light: boolean,
  sign: BotSign,
) {
  const rows = SIGN_ROWS[sign];
  if (!light) {
    context.fillStyle = "#0d0d0f99";
    rows.forEach((row, j) =>
      [...row].forEach((value, i) => {
        if (value === "#")
          context.fillRect(
            x + i * cell - 1,
            y + j * cell - 1,
            cell + 2,
            cell + 2,
          );
      }),
    );
  }
  context.fillStyle = light ? "#0d0d0f" : "#ffffff";
  rows.forEach((row, j) =>
    [...row].forEach((value, i) => {
      if (value === "#")
        context.fillRect(x + i * cell, y + j * cell, cell, cell);
    }),
  );
}
