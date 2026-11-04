// Original Anthropic sprite data; attribution is retained in assets/THIRD_PARTY_NOTICES.txt.
import {
  clawdClips,
  sameFrame,
  standing,
  swiftRound,
  type ClawdFrame,
} from "./clawd-clips";
import type { BotSign } from "./bot-state";
import { SIGN_ROWS } from "./bot-signs";
import { layHairline } from "./bot-hairline";
import { reuseCanvas } from "./scratch";
export type MarkState = "idle" | "working" | "asking" | "done";
interface RawSprite {
  w: number;
  h: number;
  palette: string[];
  base: string;
  deltas: string[];
  a0: number[];
}
export interface ClawdSprite {
  width: number;
  height: number;
  palette: string[];
  frames: Uint8Array[];
  anchorX: number;
  anchorY: number;
}
export interface SpritePicture extends ClawdFrame {
  offsetX?: number;
  offsetY?: number;
  sign?: BotSign;
  mini?: number;
  mirrored?: boolean;
}
export type ClawdSheet = Record<string, ClawdSprite>;
let loading: Promise<ClawdSheet> | undefined;
const canonical = new WeakMap<ClawdSheet, Map<string, ClawdFrame>>();
const frameKey = (frame: ClawdFrame) => `${frame.name}:${frame.index}`;
const paletteIndex = (value: string) =>
  value === "." ? 255 : parseInt(value, 36);
const rgba = (color: string) => {
  let value = color.slice(1);
  if (value.length === 3)
    value = [...value].map((digit) => digit + digit).join("");
  if (value.length === 6) value += "ff";
  return value.toLowerCase();
};

/** Literal JSON decoding only; frame bytes and coordinates match ClawdSpriteTable.swift. */
export function decodeClawdTable(table: {
  sprites: Record<string, RawSprite>;
}): ClawdSheet {
  const sheet: ClawdSheet = {};
  for (const [name, raw] of Object.entries(table.sprites)) {
    let frame = Uint8Array.from(raw.base, paletteIndex);
    const frames = [frame];
    for (const delta of raw.deltas) {
      frame = frame.slice();
      for (const change of delta.split(","))
        if (change.length >= 2)
          frame[parseInt(change.slice(0, -1), 36)] = paletteIndex(
            change.slice(-1),
          );
      frames.push(frame);
    }
    const [left, top, width, height] = raw.a0;
    sheet[name] = {
      width: raw.w,
      height: raw.h,
      palette: raw.palette,
      frames,
      anchorX: left + Math.floor(width / 2),
      anchorY: top + height,
    };
  }
  return sheet;
}
export function loadClawd(): Promise<ClawdSheet> {
  return (loading ??= fetch(
    new URL("../assets/clawd-sprites.json", import.meta.url).href,
  )
    .then(
      (response) =>
        response.json() as Promise<{ sprites: Record<string, RawSprite> }>,
    )
    .then(decodeClawdTable));
}

/** First identical anchored picture wins, with the canonical standing frame first. */
export function canonicalClawdFrame(
  sheet: ClawdSheet,
  frame: ClawdFrame,
): ClawdFrame {
  let repeats = canonical.get(sheet);
  if (!repeats) {
    repeats = new Map();
    const first = new Map<string, ClawdFrame>();
    const names = [
      "Walking",
      ...Object.keys(sheet)
        .filter((name) => name !== "Walking")
        .sort(),
    ];
    for (const name of names) {
      const sprite = sheet[name];
      if (!sprite) continue;
      const palette = sprite.palette.map(rgba);
      sprite.frames.forEach((pixels, index) => {
        const signature: string[] = [];
        pixels.forEach((ink, offset) => {
          if (ink !== 255)
            signature.push(
              `${(offset % sprite.width) - sprite.anchorX},${Math.floor(offset / sprite.width) - sprite.anchorY}:${palette[ink]}`,
            );
        });
        const key = signature.join(";"),
          previous = first.get(key),
          here = { name, index };
        if (previous) repeats!.set(frameKey(here), previous);
        else first.set(key, here);
      });
    }
    canonical.set(sheet, repeats);
  }
  return repeats.get(frameKey(frame)) ?? frame;
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
const bakers = new WeakMap<ClawdSheet, ClawdBaker>();
export function clawdBaker(sheet: ClawdSheet) {
  let baker = bakers.get(sheet);
  if (!baker) {
    baker = new ClawdBaker(sheet);
    bakers.set(sheet, baker);
  }
  return baker;
}
export function drawClawd(
  context: CanvasRenderingContext2D,
  sheet: ClawdSheet,
  picture: SpritePicture,
  side: number,
  scale: number,
  extent: number,
  light: boolean,
) {
  const image = clawdBaker(sheet).image(picture, side, scale, extent, light);
  if (image) {
    context.imageSmoothingEnabled = false;
    context.drawImage(image, 0, 0);
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

// Useful to a renderer handing off a procedural standing pose to the sprite actor.
export const isClawdStanding = (sheet: ClawdSheet, picture: SpritePicture) =>
  sameFrame(
    canonicalClawdFrame(sheet, picture),
    canonicalClawdFrame(sheet, standing),
  );
