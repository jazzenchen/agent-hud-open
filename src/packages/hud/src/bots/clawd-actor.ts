import type { BotCue, BotEvent, BotSign, BotState } from "./bot-state";
import {
  canonicalClawdFrame,
  ClawdBaker,
  type ClawdSheet,
  type SpritePicture,
} from "./clawd";
import { ClawdPerformer, type ClawdPerformerOptions } from "./clawd-performer";
import { frameAfter } from "./clawd-clips";

interface Seen {
  picture: SpritePicture | null;
  sign?: BotSign;
}
export interface ClawdActorOptions extends ClawdPerformerOptions {
  resumes?: boolean;
  restsHidden?: boolean;
}
export interface ClawdActorFrame {
  front: Seen;
  back: Seen | null;
  frontOpacity: number;
  backOpacity: number;
  opacity: number;
  visible: boolean;
  due: number;
}
const logo: Seen = { picture: null };
/** IslandAnimation.mediaCurve, ClawdActor's exact original crossfade curve. */
function fadeCurve(progress: number) {
  const x = Math.max(0, Math.min(1, progress));
  if (x === 0 || x === 1) return x;
  let low = 0,
    high = 1;
  for (let i = 0; i < 16; i++) {
    const t = (low + high) / 2;
    const bx = 3 * (1 - t) ** 2 * t * 0.2 + 3 * (1 - t) * t * t * 0.2 + t ** 3;
    if (bx < x) low = t;
    else high = t;
  }
  const t = (low + high) / 2;
  return 3 * (1 - t) ** 2 * t * 0.8 + 3 * (1 - t) * t * t + t ** 3;
}

/** Pure TS counterpart of ClawdActor's two-layer presentation; canvas resources never own its clock. */
export class ClawdActor {
  static readonly fade = 0.3;
  static readonly keepsFrames = 60;
  readonly performer: ClawdPerformer;
  baker: ClawdBaker | null = null;
  restsHidden: boolean;
  private seen: Seen | null;
  private back: Seen | null = null;
  private fadeAt = -Infinity;
  private fadesUntil = -Infinity;
  private opacityFrom = 1;
  private opacityTarget = 1;
  private opacityAt = -Infinity;
  private restingSince: number | null = null;
  private resumes: boolean;

  constructor(options: ClawdActorOptions = {}) {
    this.performer = new ClawdPerformer(options);
    this.restsHidden = options.restsHidden ?? true;
    this.resumes = options.resumes ?? false;
    this.seen = this.resumes ? null : logo;
  }
  get state() {
    return this.performer.state;
  }
  get playing() {
    return this.performer.playing;
  }
  get picture() {
    return this.performer.picture;
  }
  get switchPoint() {
    return this.performer.switchPoint;
  }
  get atSwitchPoint() {
    return this.performer.atSwitchPoint;
  }
  set(state: BotState, now: number) {
    this.performer.set(state, now);
    this.sync(now);
  }
  resume(state: BotState, now: number) {
    this.performer.resume(state, now);
    this.sync(now);
  }
  play(event: BotEvent, now: number) {
    this.performer.play(event, now);
    this.sync(now);
  }
  playCue(cue: BotCue | null | undefined, now: number) {
    this.performer.playCue(cue, now);
    this.sync(now);
  }
  useSheet(sheet: ClawdSheet) {
    if (this.baker?.sheet === sheet) return;
    this.baker = new ClawdBaker(sheet);
    this.performer.player.setCanonical((frame) =>
      canonicalClawdFrame(sheet, frame),
    );
  }
  redraw() {
    this.baker?.purge();
  }
  step(now: number, aim: readonly [number, number] = [0, 0]): ClawdActorFrame {
    this.performer.step(now, aim);
    this.sync(now);
    const front = this.seen ?? logo;
    const share = this.back
      ? fadeCurve((now - this.fadeAt) / ClawdActor.fade)
      : 1;
    const opacity =
      this.opacityAt === -Infinity
        ? this.opacityTarget
        : this.opacityFrom +
          (this.opacityTarget - this.opacityFrom) *
            fadeCurve((now - this.opacityAt) / ClawdActor.fade);
    const visible = !(
      this.restsHidden &&
      front.picture === null &&
      front.sign === undefined &&
      this.back === null &&
      opacity === 1
    );
    let due = this.performer.due;
    // CALayer animates these independently of the source sprite clock. Canvas needs paint wakes
    // for the same crossfade; sprite pictures themselves still change only on the 12fps grid.
    if (this.fadesUntil > now)
      due = Math.min(
        due,
        this.fadesUntil,
        (Math.floor((now + 0.004) * 24) + 1) / 24,
      );
    if (
      front.picture === null &&
      this.restingSince !== null &&
      this.baker &&
      this.baker.count > 0
    )
      due = Math.min(
        due,
        frameAfter(this.restingSince + ClawdActor.keepsFrames),
      );
    this.resumes = false;
    return {
      front,
      back: this.back,
      frontOpacity: share,
      backOpacity: 1 - share,
      opacity,
      visible,
      due,
    };
  }
  private sync(now: number) {
    if (now >= this.fadesUntil - 0.004) this.back = null;
    const picture = this.performer.picture;
    const next: Seen = {
      picture,
      ...(picture === null && this.performer.logoSign
        ? { sign: this.performer.logoSign }
        : {}),
    };
    if (picture === null) this.restingSince ??= now;
    else if (!["no-news", "idle", "asleep"].includes(this.state.look))
      this.restingSince = null;
    if (
      picture === null &&
      this.restingSince !== null &&
      now - this.restingSince >= ClawdActor.keepsFrames - 0.004
    )
      this.baker?.purge();
    if (this.seen && (this.seen.picture === null) !== (picture === null)) {
      this.back = this.seen;
      // The player knows when a script actually ended, even while canvas draws were suspended.
      const changedAt =
        picture === null ? this.performer.appearanceChangedAt : now;
      this.fadeAt = changedAt;
      this.fadesUntil = Math.max(
        this.fadesUntil,
        frameAfter(changedAt + ClawdActor.fade),
      );
      if (now >= this.fadesUntil - 0.004) this.back = null;
    }
    this.seen = next;
    const opacity = picture === null && this.state.look === "no-news" ? 0.5 : 1;
    if (opacity !== this.opacityTarget) {
      this.opacityFrom =
        this.opacityAt === -Infinity
          ? this.opacityTarget
          : this.opacityFrom +
            (this.opacityTarget - this.opacityFrom) *
              fadeCurve((now - this.opacityAt) / ClawdActor.fade);
      this.opacityTarget = opacity;
      if (this.resumes) {
        this.opacityFrom = opacity;
        this.opacityAt = -Infinity;
      } else {
        this.opacityAt = now;
        this.fadesUntil = Math.max(
          this.fadesUntil,
          frameAfter(now + ClawdActor.fade),
        );
      }
    }
  }
}
