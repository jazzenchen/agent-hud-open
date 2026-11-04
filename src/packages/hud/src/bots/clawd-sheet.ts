// Original Anthropic sprite data; attribution is retained in assets/THIRD_PARTY_NOTICES.txt.
// The sprite sheet: decoding the bundled table, loading it once, and the
// canonical frame each repeated picture stands for.
import type { ClawdFrame } from "./clawd-clips";
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
