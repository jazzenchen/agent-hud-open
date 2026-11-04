import { BotMotion } from "./bot-motion";
import { ClawdActor } from "./clawd-actor";
import { sameBotState, type BotState, type BotCue } from "./bot-state";
import type { BotForm } from "./bot-form";
import type { ClawdActorFrame } from "./clawd-actor";

export type BotRendererKind = "procedural" | "clawd";
export interface BotActorStep {
  stepped: boolean;
  renderer: BotRendererKind;
  due: number;
  frame: ClawdActorFrame | null;
}

export interface BotActorOptions {
  staysBot?: boolean;
  arrivesAfter?: number;
  awake?: boolean;
}
/** One event timeline; canvas resources and both renderers are consumers of it. */
export class BotActor {
  readonly motion: BotMotion;
  readonly clawd: ClawdActor;
  state: BotState;
  private cue: BotCue | null = null;
  private fitted: BotForm | null = null;
  private renderer: BotRendererKind | null = null;
  private next = 0;
  private frame: ClawdActorFrame | null = null;
  get due() {
    return this.next;
  }
  invalidate() {
    this.next = 0;
  }
  selectRenderer(renderer: BotRendererKind) {
    if (renderer !== this.renderer) {
      this.renderer = renderer;
      this.invalidate();
    }
  }
  constructor(state: BotState, now: number, options: BotActorOptions = {}) {
    this.state = state;
    this.motion = new BotMotion();
    this.motion.staysBot = options.staysBot ?? false;
    this.motion.arrivesAfter = options.arrivesAfter ?? 0;
    this.clawd = new ClawdActor({
      ...options,
      resumes: options.awake,
      restsHidden: false,
    });
    this.motion.set(state, now);
    if (options.awake) {
      this.motion.resume(now);
      this.clawd.resume(state, now);
    } else this.clawd.set(state, now);
  }
  fit(form: BotForm) {
    if (this.fitted !== form) {
      this.fitted = form;
      this.motion.fit(form.spec);
      this.invalidate();
    }
  }
  set(state: BotState, now: number) {
    if (sameBotState(this.state, state)) return false;
    this.state = state;
    this.motion.set(state, now);
    this.clawd.set(state, now);
    this.invalidate();
    return true;
  }
  playCue(cue: BotCue | null | undefined, now: number) {
    if (!cue || (this.cue?.event === cue.event && this.cue.at === cue.at))
      return false;
    this.cue = cue;
    if (now - cue.at <= 3) {
      this.motion.play(cue.event, cue.at);
      this.clawd.playCue(cue, now);
      this.invalidate();
      return true;
    }
    return false;
  }
  step(
    now: number,
    aim: readonly [number, number],
    renderer: BotRendererKind = "procedural",
  ): BotActorStep {
    this.selectRenderer(renderer);
    if (now < this.next - 0.004) {
      return { stepped: false, renderer, due: this.next, frame: this.frame };
    }
    if (renderer === "clawd") {
      this.frame = this.clawd.step(now, aim);
      this.next = this.frame.due;
    } else {
      this.motion.step(now, aim);
      this.next = this.motion.wait === null ? Infinity : now + this.motion.wait;
    }
    return { stepped: true, renderer, due: this.next, frame: this.frame };
  }
}
