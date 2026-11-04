import type { SpritePicture } from "./clawd";
import { shownSign, type BotSign } from "./bot-state";
import {
  clawdClips,
  clawdTicks,
  easedOffset,
  isClawdAttention,
  sameFrame,
  standing,
  zeroOffset,
  type ClawdCard,
  type ClawdClip,
  type ClawdEvent,
  type ClawdFrame,
  type ClawdLook,
  type ClawdOffset,
  type ClawdShot,
  type ClawdVisit,
} from "./clawd-clips";

interface Shown {
  look: ClawdLook;
  clip: ClawdClip;
  still: boolean;
  epoch: number | null;
}
interface Step {
  frame: ClawdFrame | null;
  ticks: number;
  offset: ClawdOffset;
  sign?: BotSign;
  mini?: number;
  holds?: boolean;
  endsCycle?: boolean;
  outro?: boolean;
  mirrored?: boolean;
}
type Activity =
  | { kind: "resting" | "visit" | "greeting" | "gazing" | "card" | "at-card" }
  | { kind: "entering" | "looping" | "leaving"; shown: Shown }
  | { kind: "event"; event: ClawdEvent; resume: Shown | null };
const tickAt = (time: number) => Math.floor((time + 0.004) * 12);
const frameStep = (
  frame: ClawdFrame | null,
  values: Partial<Step> = {},
): Step => ({ frame, ticks: 1, offset: zeroOffset, ...values });
const samePicture = (a: SpritePicture | null, b: SpritePicture | null) =>
  JSON.stringify(a) === JSON.stringify(b);

/** Full source ClawdPlayer: clips, not renders, own every state transition on the absolute 12fps grid. */
export class ClawdPlayer {
  readonly clips = clawdClips;
  readonly restMode: "logo" | "standing";
  private canonical = (frame: ClawdFrame) => frame;
  target: ClawdLook | null = null;
  subagents = false;
  pendingEvent: ClawdEvent | null = null;
  badge: BotSign | undefined;
  private hovered = false;
  private sector: number | null = null;
  private activity: Activity = { kind: "resting" };
  private script: Step[];
  private start = 0;
  private tick = 0;
  private cameInAt: number | null = null;
  private showing: boolean;
  private appearanceSince = 0;

  constructor(rest: "logo" | "standing") {
    this.restMode = rest;
    this.script = [this.restStep()];
    this.showing = this.restMode === "standing";
  }
  get picture(): SpritePicture | null {
    return this.pictureOf(this.script[0]);
  }
  get appearanceChangedAt() {
    return this.appearanceSince;
  }
  setCanonical(canonical: (frame: ClawdFrame) => ClawdFrame) {
    this.canonical = canonical;
  }
  get due(): number {
    return this.nextChange() ?? Infinity;
  }
  get shown(): ClawdLook | null {
    return "shown" in this.activity ? this.activity.shown.look : null;
  }
  get playing(): ClawdEvent | null {
    return this.activity.kind === "event" ? this.activity.event : null;
  }
  get switchPoint() {
    return this.switchTick / 12;
  }
  get atSwitchPoint() {
    return this.switchTick <= this.tick + 1;
  }
  private get switchTick() {
    const activity = this.activity;
    switch (activity.kind) {
      case "resting":
      case "visit":
      case "greeting":
      case "gazing":
      case "card":
      case "at-card":
        return this.tick;
      case "looping": {
        if (
          this.script[0].holds ||
          activity.shown.clip.exit.kind === "any-frame" ||
          sameFrame(this.picture, this.canonical(standing))
        )
          return this.tick + 1;
        let end = this.start;
        for (const step of this.script) {
          end += step.ticks;
          if (step.endsCycle) break;
        }
        return end;
      }
      default:
        return this.script.reduce(
          (time, step) =>
            Number.isFinite(step.ticks) ? time + step.ticks : time,
          this.start,
        );
    }
  }

  show(look: ClawdLook | null, time: number, subagents = false) {
    const now = this.stepTo(time);
    this.subagents = subagents;
    const activity = this.activity,
      atCard = activity.kind === "at-card";
    if (look === this.target && !atCard) return;
    this.target = look;
    switch (activity.kind) {
      case "resting":
      case "visit":
      case "greeting":
      case "gazing":
      case "card":
      case "at-card":
        if (look === null) {
          if (atCard) this.rest(now);
        } else {
          this.cameInAt = now;
          this.enter(look, null, now);
        }
        break;
      case "entering":
      case "looping": {
        const shown = activity.shown;
        if (look && isClawdAttention(look) && look !== shown.look)
          this.enter(look, null, now);
        else if (look && this.sameClip(look, shown)) {
          if (
            activity.kind === "looping" &&
            shown.still !== (look === "retrying")
          )
            this.endHold(now);
        } else if (activity.kind === "looping") this.leave(shown, now);
        break;
      }
      case "leaving":
        if (look && isClawdAttention(look)) this.enter(look, null, now);
        else if (
          look &&
          this.sameClip(look, activity.shown) &&
          !this.script[0].outro
        ) {
          this.activity = { kind: "looping", shown: activity.shown };
          this.script = this.script
            .filter((step) => !step.outro)
            .map((step) => ({ ...step, sign: activity.shown.clip.sign }));
        }
        break;
      case "event":
        if (look && isClawdAttention(look)) this.enter(look, null, now);
        break;
    }
  }
  resume(look: ClawdLook, time: number, subagents = false) {
    const now = this.stepTo(time);
    this.target = look;
    this.subagents = subagents;
    this.loop(
      {
        look,
        clip: this.clips.looks[look],
        still: look === "retrying",
        epoch: now,
      },
      now,
    );
  }
  play(event: ClawdEvent, time: number) {
    const now = this.stepTo(time),
      activity = this.activity;
    switch (activity.kind) {
      case "resting":
      case "visit":
      case "greeting":
      case "gazing":
      case "card":
      case "at-card":
        this.playShot(event, null, now);
        break;
      case "entering":
        if (this.cameInAt === now && !isClawdAttention(activity.shown.look))
          this.playShot(event, null, now);
        else this.pendingEvent = event;
        break;
      case "looping":
        this.pendingEvent = event;
        if (!isClawdAttention(activity.shown.look)) this.endHold(now);
        break;
      default:
        this.pendingEvent = event;
    }
  }
  cancelEvent() {
    this.pendingEvent = null;
  }
  visit(visit: ClawdVisit, time: number) {
    const now = this.stepTo(time);
    if (
      this.activity.kind !== "resting" ||
      this.target !== null ||
      this.pendingEvent !== null
    )
      return;
    this.activity = { kind: "visit" };
    this.begin(this.stepsOf(this.clips.visits[visit]), now);
  }
  greet(time: number) {
    const now = this.stepTo(time);
    this.hovered = true;
    if (
      (this.activity.kind === "resting" || this.activity.kind === "visit") &&
      this.target === null
    ) {
      this.activity = { kind: "greeting" };
      this.begin(this.stepsOf(this.clips.greet), now);
    }
  }
  endHover(time: number) {
    const now = this.stepTo(time);
    this.hovered = false;
    if (this.activity.kind === "gazing") this.rest(now);
  }
  look(aim: readonly [number, number], time: number) {
    const now = this.stepTo(time),
      sector = ClawdPlayer.sectorOf(aim);
    if (sector === null || sector === this.sector) return;
    this.sector = sector;
    if (this.activity.kind === "gazing") this.gaze(now);
  }
  card(card: ClawdCard, time: number) {
    const now = this.stepTo(time);
    this.target = null;
    this.pendingEvent = null;
    this.activity = { kind: "card" };
    this.begin(this.stepsOf(this.clips.cards[card]), now);
  }
  pointAtCard(mirrored: boolean, time: number) {
    this.holdAtCard(
      frameStep(this.clips.pointsAtCard, {
        ticks: Infinity,
        holds: true,
        mirrored,
      }),
      time,
    );
  }
  standAtCard(look: ClawdLook, time: number) {
    const clip = this.clips.looks[look],
      first = clip.loop[0];
    const held =
      clip.sign === undefined && first !== undefined
        ? frameStep(
            { name: clip.sprite, index: first },
            { ticks: Infinity, offset: clip.offset, holds: true },
          )
        : frameStep(standing, {
            ticks: Infinity,
            sign: clip.sign,
            holds: true,
          });
    this.holdAtCard(held, time);
  }
  setBadge(badge: BotSign | undefined, time: number) {
    this.stepTo(time);
    this.badge = badge;
  }
  advance(time: number) {
    this.stepTo(time);
    return this.picture;
  }

  private restStep() {
    return frameStep(this.restMode === "standing" ? standing : null, {
      ticks: Infinity,
      holds: true,
    });
  }
  private stepTo(time: number) {
    const tick = Math.max(this.tick, tickAt(time));
    this.playTo(tick);
    this.tick = tick;
    return tick;
  }
  private playTo(tick: number) {
    while (
      Number.isFinite(this.script[0].ticks) &&
      this.start + this.script[0].ticks <= tick
    ) {
      this.start += this.script[0].ticks;
      this.script.shift();
      if (!this.script.length) this.proceed(this.start);
    }
  }
  private nextChange(): number | null {
    const picture = this.picture;
    const probe = this.clone();
    for (let at = 0; at < 10000; at++) {
      if (!Number.isFinite(probe.script[0].ticks)) return null;
      const end = probe.start + probe.script[0].ticks;
      probe.playTo(end);
      if (!samePicture(probe.picture, picture)) return end / 12;
    }
    return null;
  }
  private clone() {
    const probe = new ClawdPlayer(this.restMode);
    probe.canonical = this.canonical;
    probe.target = this.target;
    probe.subagents = this.subagents;
    probe.pendingEvent = this.pendingEvent;
    probe.badge = this.badge;
    probe.hovered = this.hovered;
    probe.sector = this.sector;
    probe.activity = this.activity;
    probe.script = this.script.map((step) => ({ ...step }));
    probe.start = this.start;
    probe.tick = this.tick;
    probe.cameInAt = this.cameInAt;
    probe.showing = this.showing;
    probe.appearanceSince = this.appearanceSince;
    return probe;
  }
  private pictureOf(step: Step): SpritePicture | null {
    if (!step.frame) return null;
    const frame = this.canonical(step.frame),
      sign = shownSign(step.sign ?? null, this.badge ?? null);
    return {
      name: frame.name,
      index: frame.index,
      offsetX: step.offset.x,
      offsetY: step.offset.y,
      ...(sign ? { sign } : {}),
      ...(this.subagents && step.mini !== undefined ? { mini: step.mini } : {}),
      mirrored: step.mirrored ?? false,
    };
  }
  private sameClip(look: ClawdLook, shown: Shown) {
    return this.clips.looks[look].name === shown.clip.name;
  }
  private begin(steps: Step[], tick: number) {
    this.start = tick;
    this.script = steps;
    if (!steps.length) {
      this.proceed(tick);
      return;
    }
    const showing = steps[0].frame !== null;
    if (showing !== this.showing) {
      this.showing = showing;
      this.appearanceSince = tick / 12;
    }
  }
  private proceed(tick: number) {
    const activity = this.activity;
    switch (activity.kind) {
      case "entering":
      case "looping":
        this.reachSwitch(activity.shown, tick);
        break;
      case "leaving":
        if (
          this.pendingEvent &&
          !(this.target && isClawdAttention(this.target))
        )
          this.playShot(this.pendingEvent, null, tick);
        else if (this.target) this.enter(this.target, activity.shown, tick);
        else this.rest(tick);
        break;
      case "event":
        if (
          this.pendingEvent &&
          !(this.target && isClawdAttention(this.target))
        )
          this.playShot(this.pendingEvent, activity.resume, tick);
        else if (
          this.target &&
          activity.resume &&
          this.sameClip(this.target, activity.resume)
        )
          this.loop(
            {
              look: this.target,
              clip: activity.resume.clip,
              still: this.target === "retrying",
              epoch: activity.resume.epoch,
            },
            tick,
          );
        else if (this.target) this.enter(this.target, null, tick);
        else this.rest(tick);
        break;
      case "greeting":
        if (this.hovered) this.gaze(tick);
        else this.rest(tick);
        break;
      case "visit":
      case "card":
        if (this.target) this.enter(this.target, null, tick);
        else this.rest(tick);
        break;
      default:
        this.rest(tick);
    }
  }
  private reachSwitch(shown: Shown, tick: number) {
    if (this.pendingEvent && !isClawdAttention(shown.look)) {
      this.playShot(this.pendingEvent, shown, tick);
      return;
    }
    if (!this.target || !this.sameClip(this.target, shown)) {
      this.leave(shown, tick);
      return;
    }
    this.loop(
      { ...shown, look: this.target, still: this.target === "retrying" },
      tick,
    );
  }
  private rest(tick: number) {
    this.activity = { kind: "resting" };
    this.begin([this.restStep()], tick);
  }
  private holdAtCard(step: Step, time: number) {
    const now = this.stepTo(time);
    this.activity = { kind: "at-card" };
    this.begin([step], now);
  }
  private gaze(tick: number) {
    this.activity = { kind: "gazing" };
    const frame =
      this.sector === null
        ? standing
        : {
            name: "LookingAroundEyesOnly",
            index: this.clips.gaze[this.sector % this.clips.gaze.length],
          };
    this.begin([frameStep(frame, { ticks: Infinity, holds: true })], tick);
  }
  private playShot(event: ClawdEvent, resume: Shown | null, tick: number) {
    this.pendingEvent = null;
    this.activity = { kind: "event", event, resume };
    this.begin(this.stepsOf(this.clips.events[event]), tick);
  }
  private stepsOf(shot: ClawdShot): Step[] {
    return shot.steps.map((step) =>
      frameStep(step.frame, {
        ticks: step.ticks,
        offset: step.offset,
        sign: step.sign,
      }),
    );
  }
  private enter(look: ClawdLook, source: Shown | null, tick: number) {
    const clip = this.clips.looks[look],
      handover = source !== null && source.clip.working && clip.working;
    const frames = handover ? (clip.entry ?? clip.intro) : clip.intro;
    this.activity = {
      kind: "entering",
      shown: { look, clip, still: look === "retrying", epoch: null },
    };
    const steps = frames.map((frame, at) =>
      frameStep(frame, {
        ticks: clip.ticks,
        offset: easedOffset(
          clip.offset,
          frames.length > 1 ? at / (frames.length - 1) : 1,
        ),
        sign: clip.sign,
        mini: clip.miniShift,
      }),
    );
    if (steps.length) steps[steps.length - 1].endsCycle = true;
    this.begin(steps, tick);
  }
  private loop(shown: Shown, tick: number) {
    const next = { ...shown, epoch: shown.epoch ?? tick };
    this.activity = { kind: "looping", shown: next };
    this.begin(this.cycle(next, tick), tick);
  }
  private cycle(shown: Shown, tick: number): Step[] {
    const clip = shown.clip,
      each = clip.ticks;
    const make = (index: number, ticks: number) =>
      frameStep(
        { name: clip.sprite, index },
        { ticks, offset: clip.offset, sign: clip.sign, mini: clip.miniShift },
      );
    if (shown.still || !clip.loop.length)
      return [
        {
          ...(clip.loop.length
            ? make(clip.loop[0], Infinity)
            : frameStep(null, { ticks: Infinity })),
          holds: true,
        },
      ];
    let steps = clip.loop.map((index) => make(index, each));
    const looped = tick - (shown.epoch ?? tick),
      past = (seconds: number) => looped >= clawdTicks(seconds),
      pace = clip.pace;
    if (pace.kind === "resting" && past(pace.after)) {
      steps[0].ticks += clawdTicks(pace.hold);
      steps[0].holds = true;
    } else if (pace.kind === "waiting" && past(pace.after)) {
      steps[0].ticks = Math.max(
        each,
        clawdTicks(past(pace.slowAfter) ? pace.slowEvery : pace.every) -
          (steps.length - 1) * each,
      );
      steps[0].holds = true;
    } else if (pace.kind === "replaying" && past(pace.after)) {
      steps = [
        {
          ...make(
            pace.hold,
            Math.max(each, clawdTicks(pace.every) - pace.replay.length * each),
          ),
          holds: true,
        },
        ...pace.replay.map((index) => make(index, each)),
      ];
    }
    steps[steps.length - 1].endsCycle = true;
    return steps;
  }
  private leave(shown: Shown, tick: number) {
    const clip = shown.clip,
      empty = !this.script.length;
    let lead: Step[];
    if (empty) lead = this.exitLead(clip);
    else {
      lead = this.script.map((step) => ({ ...step }));
      if (lead[0].holds) lead[0].ticks = tick + 1 - this.start;
      switch (clip.exit.kind) {
        case "cycle-end":
          if (this.script[0].holds) lead = [lead[0]];
          break;
        case "any-frame":
          lead = [lead[0]];
          break;
        case "through": {
          const at = lead.findIndex((step) =>
            sameFrame(step.frame, {
              name: clip.sprite,
              index: clip.exit.kind === "through" ? clip.exit.frame : 0,
            }),
          );
          lead =
            at >= 0 ? lead.slice(0, at + 1) : [...lead, ...this.exitLead(clip)];
          break;
        }
      }
      lead = lead.map((step, at) =>
        at > 0 && step.holds ? { ...step, ticks: clip.ticks } : step,
      );
    }
    lead = lead.map((step, at) => ({
      ...step,
      sign: at > 0 || empty ? undefined : step.sign,
      holds: false,
      endsCycle: false,
    }));
    const outro = clip.outro.map((frame, at) =>
      frameStep(frame, {
        ticks: clip.ticks,
        offset: easedOffset(
          clip.offset,
          clip.outro.length > 1 ? 1 - at / (clip.outro.length - 1) : 0,
        ),
        mini: clip.miniShift,
        outro: true,
      }),
    );
    this.activity = { kind: "leaving", shown };
    if (empty) this.begin([...lead, ...outro], tick);
    else this.script = [...lead, ...outro];
  }
  private exitLead(clip: ClawdClip): Step[] {
    if (clip.exit.kind !== "through") return [];
    const at = clip.loop.indexOf(clip.exit.frame);
    if (at < 0) return [];
    return clip.loop
      .slice(0, at + 1)
      .map((index) =>
        frameStep(
          { name: clip.sprite, index },
          { ticks: clip.ticks, offset: clip.offset, mini: clip.miniShift },
        ),
      );
  }
  private endHold(tick: number) {
    if (this.activity.kind !== "looping" || !this.script[0].holds) return;
    this.script = [
      { ...this.script[0], ticks: tick + 1 - this.start, endsCycle: true },
    ];
  }
  static sectorOf(aim: readonly [number, number]): number | null {
    if (Math.hypot(...aim) < 0.02) return null;
    const turns =
      Math.sign(Math.atan2(-aim[1], aim[0])) *
      Math.floor(Math.abs(Math.atan2(-aim[1], aim[0]) / (Math.PI / 4)) + 0.5);
    return ((turns % 8) + 8) % 8;
  }
}
