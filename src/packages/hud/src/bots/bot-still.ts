import type { BotForm } from "./bot-form";
import type { BotFlap } from "./bot-pose";
interface Slot {
  form: BotForm;
  width: number;
  height: number;
  transform: string;
  morph: number;
  turn: number;
  helper: boolean;
  flap: string;
  bytes: ImageData | null;
}
/** Same 36-slot, second-identical-frame bitmap cache as BotStill.swift. */
export class BotStill {
  private static slots: Slot[] = [];
  private key: Omit<Slot, "bytes">;
  constructor(
    form: BotForm,
    morph: number,
    turn: number,
    helper: boolean,
    flap: BotFlap,
    private context: CanvasRenderingContext2D,
  ) {
    const t = context.getTransform();
    this.key = {
      form,
      width: context.canvas.width,
      height: context.canvas.height,
      transform: [t.a, t.b, t.c, t.d, t.e, t.f].join(","),
      morph,
      turn,
      helper,
      flap: `${flap.left},${flap.right}`,
    };
  }
  private slot() {
    const key = this.key,
      index = BotStill.slots.findIndex(
        (s) =>
          s.form === key.form &&
          s.width === key.width &&
          s.height === key.height &&
          s.transform === key.transform,
      );
    if (index < 0) return null;
    const slot = BotStill.slots.splice(index, 1)[0];
    BotStill.slots.push(slot);
    return slot;
  }
  private same(slot: Slot) {
    const k = this.key;
    return (
      slot.morph === k.morph &&
      slot.turn === k.turn &&
      slot.helper === k.helper &&
      slot.flap === k.flap
    );
  }
  restore() {
    const slot = this.slot();
    if (!slot?.bytes || !this.same(slot)) return false;
    this.context.putImageData(slot.bytes, 0, 0);
    return true;
  }
  keep() {
    const slot = this.slot();
    if (!slot) {
      if (BotStill.slots.length >= 36) BotStill.slots.shift();
      BotStill.slots.push({ ...this.key, bytes: null });
      return;
    }
    if (!this.same(slot)) {
      Object.assign(slot, this.key, { bytes: null });
      return;
    }
    slot.bytes = this.context.getImageData(
      0,
      0,
      this.key.width,
      this.key.height,
    );
  }
  static forget() {
    this.slots = [];
  }
}
