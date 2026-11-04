import {
  botState,
  botPhaseState,
  isAttention,
  isWorking,
  isCalm,
  staysInPlace,
  eventRank,
  badgeSign,
  shownSign,
  stillSign,
  facingAim,
  BOT_EVENT_FRESHNESS,
  type BotState,
  type BotPhase,
  type BotEvent,
  type BotSign,
} from "./bot-state";
import { BotMoves, easeInOut, easeOut, easeOutBack } from "./bot-moves";
export interface BotFace {
  open: number;
  look: number;
  lookY: number;
  wide: number;
  happy: number;
  lean: number;
  crossed: number;
}
export interface BotPose {
  x: number;
  y: number;
  rotation: number;
  scaleX: number;
  scaleY: number;
  footing: number;
}
export interface BotLimbs {
  left: number;
  right: number;
  lifted: number;
  x: number;
  y: number;
  spinner: number;
}
export interface BotFlap {
  left: number;
  right: number;
}
export const botFace = (): BotFace => ({
  open: 1,
  look: 0,
  lookY: 0,
  wide: 1,
  happy: 0,
  lean: 0,
  crossed: 0,
});
export const botPose = (): BotPose => ({
  x: 0,
  y: 0,
  rotation: 0,
  scaleX: 1,
  scaleY: 1,
  footing: 0,
});
export const botLimbs = (): BotLimbs => ({
  left: 0,
  right: 0,
  lifted: 0,
  x: 0,
  y: 0,
  spinner: 0,
});
export function mixPose(a: BotPose, b: BotPose, t: number): BotPose {
  return Object.fromEntries(
    Object.keys(a).map((key) => [
      key,
      a[key as keyof BotPose] +
        (b[key as keyof BotPose] - a[key as keyof BotPose]) * t,
    ]),
  ) as unknown as BotPose;
}
export const BotPace = {
  lively: 1 / 24,
  following: 1 / 12,
  calm: 1 / 6,
  slack: 0.004,
  next: (now: number, pace: number) =>
    (Math.floor((now + 0.004) / pace) + 1) * pace,
  frame(time: number, now: number) {
    return Math.max(
      this.next(now, this.lively),
      Math.ceil((time - this.slack) / this.lively) * this.lively,
    );
  },
  onFrame(now: number) {
    const frame = Math.round(now / this.lively) * this.lively;
    return Math.abs(frame - now) <= this.slack ? frame : now;
  },
};
export class SplitMix {
  state: bigint;
  constructor(
    seed: bigint | number = BigInt(
      Math.floor(Math.random() * Number.MAX_SAFE_INTEGER),
    ),
  ) {
    this.state = BigInt.asUintN(64, BigInt(seed));
  }
  next() {
    this.state = BigInt.asUintN(64, this.state + 0x9e3779b97f4a7c15n);
    let z = this.state;
    z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
    z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
    return z ^ (z >> 31n);
  }
  unit() {
    return Number(this.next() >> 11n) / 2 ** 53;
  }
}
type ActKind =
  | "glance"
  | "blink-twice"
  | "squint"
  | "widen"
  | "smile"
  | "inspect"
  | "tilt"
  | "stretch"
  | "wiggle"
  | "turn"
  | "wave"
  | "scuttle"
  | "hop";
interface Act {
  kind: ActKind;
  start: number;
  length: number;
  side: number;
  gaze: readonly [number, number];
  from: number;
}
export interface MotionSpec {
  radius: number;
  spin?: number;
  limbs?: { pixel: number } | null;
  crossesEyes?: boolean;
  keeps?: unknown;
  decor?: readonly { kind: string; fins?: readonly { hand: number }[] }[];
}
const hopPath = [
  [1, 3, 0],
  [-2, 0, 1],
  [-3, 0, 2],
  [-3, 0, 2],
  [-2, 0, 1],
  [0, 0, 0],
  [1, 3, 0],
  [0, 0, 0],
] as const;
/** Direct BotMotion.swift state machine. set/play record events even when a host skips drawing. */
export class BotMotion {
  staysBot = false;
  arrivesAfter = 0;
  period = 3;
  spin = 0;
  limbed = false;
  stepShare = 0;
  crossesEyes = false;
  pulses = false;
  finned = false;
  state = botState();
  since = -10;
  morph = 0;
  face = botFace();
  pose = botPose();
  turn = 0;
  kept = { lift: 0, scale: 1 };
  limbs = botLimbs();
  flap: BotFlap = { left: 0, right: 0 };
  sign: BotSign | null = null;
  lit = 0;
  opacity = 1;
  wait: number | null = BotPace.lively;
  private last: number | null = null;
  private leaving: BotPose | null = null;
  private act: Act | null = null;
  private nextAct = 0;
  private blinks = [-1, -1];
  private nextBlink = 2;
  private following = false;
  private blinking = false;
  private blinksOn = true;
  private botInterval: readonly [number, number] | null = null;
  private greets = false;
  private greetAfter: number | null = null;
  private greeting: number | null = null;
  private greetSide = 1;
  private pending: { kind: BotEvent; asked: number } | null = null;
  private event: { kind: BotEvent; start: number } | null = null;
  private beatsFrom = 0;
  private heldSide = 1;
  private settled = false;
  private random: SplitMix;
  constructor(seed?: bigint | number) {
    this.random = new SplitMix(seed);
  }
  fit(spec: MotionSpec) {
    this.spin = spec.spin ?? 0;
    this.limbed = Boolean(spec.limbs);
    this.stepShare = spec.limbs ? (spec.limbs.pixel / 2) * spec.radius : 0;
    this.crossesEyes = Boolean(spec.crossesEyes);
    this.pulses = Boolean(spec.keeps);
    this.finned = Boolean(
      spec.decor?.some(
        (item) =>
          item.kind === "fins" && item.fins?.some((fin) => fin.hand !== 0),
      ),
    );
  }
  get look() {
    return this.state.look;
  }
  get helper() {
    return this.state.subagents && isWorking(this.look);
  }
  get isAtRest() {
    if (this.pending || this.event || this.greeting !== null) return false;
    if (this.settled) return true;
    if (this.morph !== 0 || this.staysBot || this.state.hovered) return false;
    if (this.look === "idle" || this.look === "asleep")
      return this.botInterval === null && this.poseIsRest();
    if (this.look === "no-news") return this.poseIsRest();
    return this.look === "done" && (this.last ?? 0) - this.since > 2.4;
  }
  private poseIsRest() {
    const p = this.pose;
    return (
      p.x === 0 &&
      p.y === 0 &&
      p.rotation === 0 &&
      p.scaleX === 1 &&
      p.scaleY === 1 &&
      p.footing === 0
    );
  }
  set(next: BotState | BotPhase, now: number) {
    if (typeof next === "string") next = botPhaseState(next);
    const before = this.state;
    this.state = { ...next };
    this.settled = false;
    if (next.holds && !before.holds) this.heldSide = BotMoves.ponderSide(now);
    if (next.look !== before.look) {
      this.leaving = { ...this.pose };
      this.since =
        now +
        (this.staysBot && this.morph === 0
          ? Math.max(0, this.arrivesAfter)
          : 0);
      this.beatsFrom =
        Math.ceil((this.since - BotPace.slack) / BotMoves.beat) * BotMoves.beat;
      if (isWorking(next.look))
        this.nextAct = now + 0.9 + this.random.unit() * 1.2;
      this.botInterval = null;
    }
    if (next.hovered !== before.hovered) this.hover(next.hovered, now);
    if (this.visits && this.botInterval === null)
      this.scheduleVisit(now + this.morph * 0.5);
  }
  play(next: BotEvent, now: number) {
    if (this.pending && eventRank(this.pending.kind) > eventRank(next)) return;
    this.pending = { kind: next, asked: now };
    this.settled = false;
  }
  resume(now: number) {
    if (!isWorking(this.look) && !isAttention(this.look)) return;
    this.morph = 1;
    this.leaving = null;
    this.since = now - (isAttention(this.look) ? BotMoves.firstMove : 0.3);
    this.beatsFrom =
      Math.ceil((this.since - BotPace.slack) / BotMoves.beat) * BotMoves.beat;
  }
  step(time: number, aim: readonly [number, number] = [0, 0]) {
    const now = BotPace.onFrame(time),
      gap = Math.max(0, now - (this.last ?? now)),
      dt = gap > 1 / 3 ? 0 : Math.min(1 / 6, gap);
    this.last = now;
    const age = now - this.since;
    if (this.visits && this.botInterval && now >= this.botInterval[1])
      this.scheduleVisit(now + 0.5);
    const visiting =
        this.visits &&
        this.botInterval !== null &&
        now >= this.botInterval[0] &&
        now < this.botInterval[1],
      dozing = this.look === "asleep" && !this.state.hovered,
      arrived = age >= 0;
    if (this.pending && now - this.pending.asked > BOT_EVENT_FRESHNESS)
      this.pending = null;
    if (
      this.event &&
      now - this.event.start >= BotMoves.length(this.event.kind, this.limbed)
    )
      this.event = null;
    const awake =
        arrived &&
        (isWorking(this.look) ||
          isAttention(this.look) ||
          visiting ||
          this.state.hovered ||
          (this.look === "done" && age < 1.5) ||
          this.pending !== null ||
          this.event !== null),
      wanted = arrived && (awake || this.staysBot) ? 1 : 0,
      stride = dt / 0.5;
    this.morph =
      Math.abs(wanted - this.morph) <= stride
        ? wanted
        : this.morph + (wanted > this.morph ? stride : -stride);
    this.opacity = this.look === "no-news" ? 0.5 + 0.5 * this.morph : 1;
    if (
      this.pending &&
      !this.event &&
      !isAttention(this.look) &&
      this.morph > 0.9
    ) {
      this.event = { kind: this.pending.kind, start: now };
      this.pending = null;
      this.act = null;
    }
    if (
      this.greetAfter !== null &&
      now >= this.greetAfter &&
      this.morph > 0.9 &&
      !this.event &&
      !this.pending
    ) {
      this.greetAfter = null;
      if (this.greetsOnHover) {
        this.greeting = now;
        this.greetSide = aim[0] < 0 ? -1 : 1;
      }
    }
    if (
      this.greeting !== null &&
      now - this.greeting >=
        (this.finned
          ? Math.max(BotMoves.hopSeconds, BotMoves.finWaveSeconds)
          : BotMoves.hopSeconds)
    )
      this.greeting = null;
    if (this.act && now - this.act.start >= this.act.length) {
      if (this.act.kind === "turn")
        this.turn = this.act.from + 60 * this.act.side;
      this.act = null;
    }
    const inPlace = staysInPlace(this.look);
    if (inPlace && this.acts) {
      if (
        (awake || this.staysBot) &&
        this.morph > 0.9 &&
        !this.act &&
        now >= this.nextAct
      ) {
        const chosen = this.pick(now);
        if (chosen.kind === "wave" && aim[0] !== 0)
          chosen.side = aim[0] > 0 ? 1 : -1;
        this.act = chosen;
        this.nextAct = now + chosen.length + 0.5 + this.random.unit() * 1.9;
        if (chosen.kind === "blink-twice") this.blinks = [now, now + 0.24];
      }
    } else this.act = null;
    const progress = this.act ? (now - this.act.start) / this.act.length : 0,
      swell = this.act
        ? easeInOut(
            Math.min(1, Math.min(progress / 0.22, (1 - progress) / 0.22)),
          )
        : 0;
    if (inPlace && this.morph > 0.5 && this.spin !== 0)
      this.turn =
        this.act?.kind === "turn"
          ? this.act.from + 60 * this.act.side * easeInOut(progress)
          : this.turn + this.spinRate(now, dozing) * dt;
    this.signs(now, arrived, dozing);
    this.pose = arrived
      ? this.sitting(age, progress, swell, dozing)
      : botPose();
    if (this.leaving && arrived && age < 0.3)
      this.pose = mixPose(this.leaving, this.pose, easeOut(age / 0.3));
    if (inPlace) {
      this.pose = mixPose(botPose(), this.pose, this.morph);
      this.pose.x = 0;
      this.pose.y = 0;
      this.pose.footing = 0;
      if (!this.limbed && this.look.startsWith("tool-") && !this.state.holds)
        this.pose.y = BotMoves.bob(now - this.beatsFrom) * this.morph;
    } else if (this.morph > 0.5) this.pose.x += 0.025 * this.face.look;
    if (!this.limbed) this.played(now);
    this.limbs =
      this.limbed && arrived ? this.stance(age, now, dozing) : botLimbs();
    this.flap =
      this.finned && arrived ? this.flapping(age, now) : { left: 0, right: 0 };
    this.pose.x += this.limbs.x * this.stepShare * this.morph;
    this.pose.y += this.limbs.y * this.stepShare * this.morph;
    if (arrived && (!inPlace || this.morph > 0))
      this.updateFace(age, now, dt, aim, swell, dozing);
    else this.following = this.blinking = false;
    if (arrived) this.keep(age, now, dt);
    const stills =
      (this.state.holds && isWorking(this.look)) ||
      (this.turnsToCard && age >= 0.45);
    this.settled =
      stills &&
      !this.state.hovered &&
      this.morph === wanted &&
      !this.following &&
      !this.blinking &&
      !this.act &&
      !this.event &&
      !this.pending &&
      this.greeting === null;
    const wait = this.nextFrame(now, age, wanted, dozing);
    this.wait = wait === null ? null : wait + now - time;
  }
  private nextFrame(
    now: number,
    age: number,
    wanted: number,
    dozing: boolean,
  ): number | null {
    if (this.isAtRest) return null;
    const at = (time: number) => BotPace.frame(time, now) - now,
      every = (pace: number) => BotPace.next(now, pace) - now;
    if (age < 0) return at(this.since);
    if (this.morph === 0 && wanted === 0) {
      if (this.visits && this.botInterval) return at(this.botInterval[0]);
      if (this.look === "done") return at(this.since + 2.41);
      return every(BotPace.lively);
    }
    if (
      this.morph !== wanted ||
      this.act ||
      this.blinking ||
      age < 0.3 ||
      this.event ||
      this.pending ||
      this.greeting !== null
    )
      return every(BotPace.lively);
    let pace: number | null = this.following
        ? BotPace.following
        : this.breathes(dozing) || this.spinRate(now, dozing) !== 0
          ? BotPace.calm
          : null,
      next = Infinity;
    if (this.blinksOn) next = Math.min(next, this.nextBlink);
    if (this.acts) next = Math.min(next, this.nextAct);
    if (this.greetAfter !== null) next = Math.min(next, this.greetAfter);
    if (this.visits && this.botInterval)
      next = Math.min(next, this.botInterval[1]);
    if (this.turnsToCard) {
      if (age < 0.45) return every(BotPace.lively);
    } else if (isAttention(this.look)) {
      if (age < BotMoves.firstMove || this.moving(age))
        return every(BotPace.lively);
      if (this.pulses && age < BotMoves.fullWait) pace = BotPace.following;
      next = Math.min(next, this.since + BotMoves.move(age).next);
    } else if (this.look === "done") {
      if (age < 1.5) return every(BotPace.lively);
    } else if (
      this.look.startsWith("tool-") &&
      !this.limbed &&
      !this.state.holds
    ) {
      if (BotMoves.inBurst(now - this.beatsFrom))
        pace = this.following ? BotPace.following : null;
      next = Math.min(
        next,
        this.beatsFrom + BotMoves.nextBob(now - this.beatsFrom),
      );
    } else if (
      this.look === "thinking" &&
      !this.state.holds &&
      !this.state.hovered
    ) {
      next = Math.min(next, BotMoves.nextPonder(now));
      if (this.sign === "ellipsis")
        next = Math.min(next, (Math.floor(now * 3 + 1e-6) + 1) / 3);
    }
    if (this.limbed && this.stanceBeats)
      next = Math.min(next, BotPace.next(now, BotMoves.beat));
    const byPace = pace === null ? null : every(pace),
      byTime = Number.isFinite(next) ? at(next) : null;
    return byPace === null ? byTime : Math.min(byPace, byTime ?? byPace);
  }
  private get visits() {
    return (
      !this.staysBot &&
      !this.state.hovered &&
      (this.look === "idle" || this.look === "asleep")
    );
  }
  private get acts() {
    if (this.event || this.greeting !== null || this.state.holds) return false;
    if (this.state.hovered) return staysInPlace(this.look);
    return (
      this.look === "plain" ||
      this.look.startsWith("tool-") ||
      this.look === "idle" ||
      this.look === "no-news"
    );
  }
  private get greetsOnHover() {
    return (
      this.state.hovered &&
      !isAttention(this.look) &&
      (isCalm(this.look) || this.state.badge !== null)
    );
  }
  private get turnsToCard() {
    return this.state.facing !== null && isAttention(this.look);
  }
  private breathes(dozing: boolean) {
    return (
      !this.limbed && staysInPlace(this.look) && !this.state.holds && !dozing
    );
  }
  private spinRate(now: number, dozing: boolean) {
    if (
      this.spin === 0 ||
      !staysInPlace(this.look) ||
      this.morph <= 0.5 ||
      this.state.holds ||
      dozing
    )
      return 0;
    return this.look.startsWith("tool-")
      ? BotMoves.inBurst(now - this.beatsFrom)
        ? BotMoves.busySpin
        : 0
      : this.spin;
  }
  private moving(age: number) {
    const start = BotMoves.move(age).start;
    return (
      !this.turnsToCard &&
      start !== null &&
      age - start < BotMoves.moveLength(this.look)
    );
  }
  private hover(on: boolean, now: number) {
    this.greetAfter = null;
    this.botInterval = null;
    if (!on) {
      this.greets = false;
      return;
    }
    if (this.limbed) {
      this.greets = true;
      this.act = null;
      this.nextAct = now;
    } else {
      this.greetAfter = now + BotMoves.hopSeconds;
      this.nextAct = now + 0.9 + this.random.unit() * 1.2;
    }
  }
  private scheduleVisit(now: number) {
    const start =
      now +
      (this.look === "asleep"
        ? 180 + this.random.unit() * 120
        : 20 + this.random.unit() * 15);
    this.botInterval = [start, start + 3 + this.random.unit() * 2 - 0.5];
    this.nextAct = start + 0.5;
  }
  private sitting(
    age: number,
    progress: number,
    swell: number,
    dozing: boolean,
  ) {
    const pose = botPose();
    if (isAttention(this.look)) {
      if (age < 0.45) {
        const x = age / 0.45,
          arc = Math.sin(Math.PI * x);
        pose.y = -0.26 * 4 * x * (1 - x);
        pose.scaleY = 1 + 0.08 * arc;
        pose.scaleX = 1 - 0.05 * arc;
      }
      const raised = 1 + 0.12 * easeOutBack(Math.min(1, age / 0.35));
      pose.scaleX *= raised;
      pose.scaleY *= raised;
      if (!this.turnsToCard) {
        const start = BotMoves.move(age).start;
        if (!this.limbed) {
          if (this.look === "permission")
            pose.rotation = start === null ? 0 : BotMoves.knock(age - start);
          if (this.look === "question")
            pose.rotation = BotMoves.questionTilt(age);
          if (this.look === "plan")
            pose.y += start === null ? 0 : BotMoves.nod(age - start);
        }
      }
    } else if (this.look === "done") {
      if (age < 0.6) {
        const pop = Math.sin((Math.PI * age) / 0.6);
        pose.scaleX = pose.scaleY = 1 + 0.2 * pop;
        pose.y = -0.1 * pop;
      }
    } else {
      if (this.breathes(dozing)) {
        const breath = (2 * Math.PI * age) / this.period;
        pose.scaleX = 1 + 0.03 * Math.sin(2 * breath);
        pose.scaleY = 1 - 0.03 * Math.sin(2 * breath);
      }
      if (this.look === "thinking" && !this.limbed)
        pose.rotation =
          BotMoves.ponderTilt *
          Math.max(-1, Math.min(1, this.face.look / BotMoves.ponderGaze[0]));
    }
    if (this.act && !this.limbed) {
      if (this.act.kind === "tilt") pose.rotation += this.act.side * 9 * swell;
      if (this.act.kind === "stretch") {
        const arc = Math.sin(Math.PI * progress);
        pose.scaleY *= 1 + 0.06 * arc;
        pose.scaleX *= 1 - 0.04 * arc;
      }
      if (this.act.kind === "wiggle")
        pose.rotation +=
          8 * Math.sin(2 * Math.PI * 3 * progress) * (1 - progress);
    }
    return pose;
  }
  private played(now: number) {
    if (this.greeting !== null)
      this.pose.y += BotMoves.hop(now - this.greeting) * this.morph;
    if (!this.event) return;
    const time = now - this.event.start;
    if (this.event.kind === "start")
      this.pose.y += BotMoves.hop(time) * this.morph;
    else if (this.event.kind === "failed") {
      const sag = BotMoves.sag(time);
      this.pose.x += BotMoves.shake(time) * this.morph;
      this.pose.rotation += 6 * sag * this.morph;
      this.pose.y += 0.02 * sag * this.morph;
    } else {
      const sigh = BotMoves.sigh(time);
      this.pose.scaleX *= 1 + (sigh.scaleX - 1) * this.morph;
      this.pose.scaleY *= 1 + (sigh.scaleY - 1) * this.morph;
      this.pose.footing = 0.5;
    }
  }
  private flapping(age: number, now: number): BotFlap {
    let left = 0,
      right = 0;
    if (this.event) {
      left = right =
        this.event.kind === "start"
          ? BotMoves.finFlap(now - this.event.start) * this.morph
          : 0;
    } else if (this.greeting !== null) {
      const wave = BotMoves.finWave(now - this.greeting) * this.morph;
      if (this.greetSide < 0 || this.helper) left = wave;
      else right = wave;
    } else if (this.look.startsWith("tool-") && !this.state.holds)
      left = right =
        BotMoves.bob(now - this.beatsFrom) > 0
          ? BotMoves.paddle * this.morph
          : 0;
    else if (this.look === "done")
      left = right = BotMoves.cheer(age) * this.morph;
    return { left: Math.round(left), right: Math.round(right) };
  }
  private updateFace(
    age: number,
    now: number,
    dt: number,
    aim: readonly [number, number],
    swell: number,
    dozing: boolean,
  ) {
    const target = botFace();
    let every: readonly [number, number] | null = [3.2, 6.5];
    if (isAttention(this.look)) {
      target.wide = 1.22;
      if (this.state.facing) {
        const a = facingAim(this.state.facing);
        target.look = a[0];
        target.lookY = a[1];
      } else if (this.state.hovered) {
        target.look = aim[0];
        target.lookY = aim[1];
      }
      if (this.look === "plan" && this.moving(age)) target.lookY = 0.5;
      every =
        age < 1.5 || age >= BotMoves.fullWait || this.turnsToCard
          ? null
          : [4, 7];
    } else if (this.look === "done") {
      if (age < 1.25) {
        target.happy = 1;
        every = null;
      } else {
        target.look = aim[0];
        target.lookY = aim[1];
      }
    } else if (this.look === "thinking" && !this.state.hovered) {
      const side = this.state.holds ? this.heldSide : BotMoves.ponderSide(now);
      target.look = BotMoves.ponderGaze[0] * side;
      target.lookY = BotMoves.ponderGaze[1];
      every = [2.2, 4.5];
    } else if (dozing || (this.look === "compacting" && this.limbed)) {
      target.open = 0;
      every = null;
    } else {
      target.look = aim[0];
      target.lookY = aim[1];
      every = [2.2, 4.5];
      const act = this.act;
      if (act) {
        if (act.kind === "glance") {
          target.look += (act.gaze[0] * 0.8 - target.look) * swell;
          target.lookY += (act.gaze[1] - target.lookY) * swell;
        }
        if (act.kind === "squint") target.open = 1 - 0.55 * swell;
        if (act.kind === "widen" || act.kind === "hop")
          target.wide = 1 + 0.2 * swell;
        if (act.kind === "smile") target.happy = swell > 0.4 ? 1 : 0;
        if (act.kind === "inspect") {
          target.lean = act.side * swell;
          target.look += (act.side * 0.6 - target.look) * swell;
          target.lookY -= target.lookY * swell;
        }
        if (act.kind === "stretch" && this.limbed)
          target.open = 1 - 0.9 * swell;
      }
    }
    if (this.state.holds) every = null;
    if (this.pending?.kind === "start") target.open = 0;
    if (this.event) {
      const time = now - this.event.start;
      if (this.event.kind === "start")
        target.open = Math.min(1, time / BotMoves.hopSeconds);
      else if (this.event.kind === "failed") {
        if (this.crossesEyes) target.crossed = 1;
        else target.open = 0.45;
        target.lookY = 0.35 * BotMoves.sag(time);
      } else {
        target.open = this.limbed
          ? time < 4 * BotMoves.beat
            ? 0
            : 0.5
          : BotMoves.sigh(time).open;
        target.lookY = 0.3;
      }
      every = null;
    }
    this.blinksOn = every !== null;
    const eyes = 1 - Math.exp(-dt * 18),
      head = 1 - Math.exp(-dt * 9);
    this.face.look += (target.look - this.face.look) * head;
    this.face.lookY += (target.lookY - this.face.lookY) * head;
    for (const key of ["wide", "happy", "lean", "crossed"] as const)
      this.face[key] += (target[key] - this.face[key]) * eyes;
    let lid = target.open;
    if (every) {
      if (now >= this.nextBlink) {
        this.blinks = [now, -1];
        this.nextBlink =
          now + every[0] + this.random.unit() * (every[1] - every[0]);
      }
    } else if (this.nextBlink < now) this.nextBlink = now + 1.2;
    for (const start of this.blinks) {
      const blink = (now - start) / 0.17;
      if (blink >= 0 && blink < 1) lid *= 1 - Math.sin(Math.PI * blink);
    }
    this.face.open += (lid - this.face.open) * (1 - Math.exp(-dt * 60));
    this.blinking =
      Math.abs(lid - this.face.open) > 0.01 ||
      this.blinks.some((start) => now >= start && now - start < 0.17);
    this.following =
      Math.abs(target.look - this.face.look) > 0.004 ||
      Math.abs(target.lookY - this.face.lookY) > 0.004 ||
      Math.abs(target.wide - this.face.wide) > 0.004 ||
      Math.abs(target.happy - this.face.happy) > 0.01 ||
      Math.abs(target.lean - this.face.lean) > 0.01 ||
      Math.abs(target.crossed - this.face.crossed) > 0.01;
  }
  private keep(age: number, now: number, dt: number) {
    if (staysInPlace(this.look)) {
      this.kept = { lift: 0, scale: 1 };
      return;
    }
    let lift = 0,
      scale = 1;
    if (isAttention(this.look)) {
      lift = 0.03;
      scale = 1.22;
      if (!this.turnsToCard && (age < BotMoves.fullWait || this.moving(age)))
        scale += 0.08 * Math.sin((2 * Math.PI * now) / 0.8);
    } else if (this.look === "done" && age < 0.6)
      lift = 0.22 * Math.sin((Math.PI * age) / 0.6);
    const ease = 1 - Math.exp(-dt * 22);
    this.kept.lift += (lift - this.kept.lift) * ease;
    this.kept.scale += (scale - this.kept.scale) * ease;
  }
  private signs(now: number, arrived: boolean, dozing: boolean) {
    // Arrived, the still mark's sign; on its way, only its badge's.
    let sign = arrived
      ? stillSign(this.state)
      : this.state.badge
        ? badgeSign(this.state.badge)
        : null;
    if (this.event?.kind === "failed") sign = shownSign(sign, "cross");
    if (this.morph > 0 && this.look === "thinking" && !this.limbed)
      sign = shownSign(sign, "ellipsis");
    if (this.morph > 0 && dozing) sign = shownSign(sign, "z");
    this.sign = sign;
    this.lit =
      sign === "ellipsis" && !this.state.holds
        ? Math.floor(now * 3 + 1e-6) % 3
        : 0;
  }
  private stance(age: number, now: number, dozing: boolean) {
    if (this.event)
      return BotMotion.eventStance(this.event.kind, now - this.event.start);
    const limbs = botLimbs(),
      beat = (seconds: number, every: number) =>
        Math.floor(Math.max(0, seconds) / every + 1e-6),
      clock = now - this.beatsFrom;
    if (
      this.look === "thinking" ||
      this.look.startsWith("tool-") ||
      this.look === "plain"
    ) {
      if (!this.sign) {
        const frame = this.state.holds ? 0 : beat(now, BotMoves.beat) % 10;
        limbs.spinner = frame < 6 ? frame + 1 : 11 - frame;
      }
      if (
        this.look !== "thinking" &&
        !this.state.holds &&
        BotMoves.inBurst(clock)
      ) {
        const tap = beat(clock, BotMoves.beat);
        limbs.left = tap % 2;
        limbs.right = 1 - limbs.left;
        limbs.y = Math.floor(tap / 2) % 2;
      }
    } else if (this.look === "compacting") {
      limbs.left = limbs.right = 1;
      limbs.lifted = 3;
    } else if (this.turnsToCard) {
      if (this.state.facing === "left") limbs.left = 2;
      else if (this.state.facing === "right") limbs.right = 2;
    } else if (isAttention(this.look)) {
      const move = BotMoves.move(age),
        into = move.start === null ? null : age - move.start;
      if (age < BotMoves.firstMove || this.moving(age)) {
        limbs.left = 1 + (beat(clock, BotMoves.beat) % 2);
        limbs.right = 3 - limbs.left;
        if (into !== null && beat(into, BotMoves.beat) % 2 === 0) {
          limbs.y = -2;
          limbs.lifted = 3;
        }
      } else limbs.right = 2;
    } else if (this.look === "done" && age < 1.5) {
      const step = beat(clock, 2 * BotMoves.beat);
      limbs.left = step % 2 === 0 ? 2 : 0;
      limbs.right = 2 - limbs.left;
      if (step % 2 === 1) {
        limbs.y = -2;
        limbs.lifted = 3;
      }
    } else if (this.look === "asleep" && dozing) {
      limbs.lifted = 3;
      limbs.y = 1;
    }
    const act = this.act;
    if (!act || !staysInPlace(this.look)) return limbs;
    const since = now - act.start,
      ending = since > act.length - 0.15,
      frame = (count: number) =>
        Math.min(count - 1, beat(since, act.length / count));
    if (["wave", "scuttle", "hop", "stretch"].includes(act.kind))
      limbs.left = limbs.right = limbs.y = 0;
    if (act.kind === "wave") {
      if (since < 0.24) limbs.y = -1;
      const arm =
        since < 0.12 || ending ? 1 : 2 - (beat(since - 0.12, 0.16) % 2);
      if (act.side < 0) limbs.left = arm;
      else limbs.right = arm;
    }
    if (act.kind === "scuttle") {
      const path = [0, 1, 2, 1, 0, -1, -2, -1, 0],
        step = frame(path.length);
      limbs.x = path[step] * (act.side < 0 ? -1 : 1);
      limbs.lifted =
        step === 0 || step === path.length - 1 ? 0 : 1 + (step % 2);
    }
    if (act.kind === "hop") {
      const at = hopPath[frame(hopPath.length)];
      limbs.y = at[0];
      limbs.lifted = at[1];
      limbs.left = limbs.right = at[2];
    }
    if (act.kind === "stretch" && since > 0.15 && !ending) {
      limbs.left = limbs.right = 2;
      limbs.y = -1;
    }
    return limbs;
  }
  private static eventStance(event: BotEvent, time: number) {
    const limbs = botLimbs(),
      beat = (every: number) => Math.floor(Math.max(0, time) / every + 1e-6);
    if (event === "start") {
      const at = hopPath[Math.min(hopPath.length - 1, beat(BotMoves.beat))];
      limbs.y = at[0];
      limbs.lifted = at[1];
      limbs.left = limbs.right = at[2];
    } else if (event === "failed") {
      if (time < 0.5) limbs.x = beat(1 / 12) % 2 === 0 ? 1 : -1;
      else {
        limbs.y = 1;
        limbs.lifted = 3;
      }
    } else if (time < 4 * BotMoves.beat) {
      limbs.left = limbs.right = 2;
      limbs.y = -1;
    } else {
      limbs.y = 1;
      limbs.lifted = 3;
    }
    return limbs;
  }
  private get stanceBeats() {
    return (
      !this.state.holds &&
      (this.look.startsWith("tool-") ||
        this.look === "plain" ||
        (this.look === "thinking" && this.sign === null))
    );
  }
  private pick(now: number): Act {
    let pool: [ActKind, number][] = [
      ["glance", 5],
      ["blink-twice", 2],
      ["squint", 2],
      ["widen", 2],
      ["smile", 2],
      ["inspect", 2],
      ["tilt", 3],
      ["stretch", 2],
      ["wiggle", 2],
    ];
    if (this.spin !== 0) pool.push(["turn", 3]);
    if (this.limbed) {
      pool = pool.filter(([kind]) => kind !== "tilt" && kind !== "wiggle");
      if (this.state.hovered)
        pool.push(["wave", 4], ["hop", 2], ["scuttle", 2]);
      else if (isWorking(this.look))
        pool.push(["scuttle", 4], ["hop", 3], ["wave", 1]);
      else if (isCalm(this.look))
        pool = [
          ["scuttle", 3],
          ["hop", 3],
          ["stretch", 3],
          ["wave", 2],
        ];
    }
    let roll = this.random.unit() * pool.reduce((a, b) => a + b[1], 0),
      kind = pool[0][0];
    for (const [candidate, weight] of pool) {
      roll -= weight;
      if (roll < 0) {
        kind = candidate;
        break;
      }
    }
    if (this.greets) {
      this.greets = false;
      if (this.greetsOnHover) kind = "wave";
    }
    const lengths: Record<ActKind, number> = {
        glance: 0,
        "blink-twice": 0.5,
        squint: 1.2,
        widen: 0.9,
        smile: 1.1,
        inspect: 1.5,
        tilt: 1.1,
        stretch: 0.55,
        wiggle: 0.6,
        turn: 0.7,
        wave: 1.4,
        scuttle: 1.1,
        hop: 0.8,
      },
      length =
        kind === "glance" ? 1.1 + this.random.unit() * 0.7 : lengths[kind],
      angle = this.random.unit() * 2 * Math.PI;
    return {
      kind,
      start: now,
      length,
      side: this.random.unit() < 0.5 ? -1 : 1,
      gaze: [Math.cos(angle), Math.sin(angle) * 0.7],
      from: this.turn,
    };
  }
}
