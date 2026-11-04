import {
  botState,
  botPhaseState,
  isAttention,
  isWorking,
  isCalm,
  staysInPlace,
  eventRank,
  BOT_EVENT_FRESHNESS,
  type BotState,
  type BotPhase,
  type BotEvent,
  type BotSign,
} from "./bot-state";
import { BotMoves, easeInOut, easeOut } from "./bot-moves";
import { BotPace, SplitMix } from "./bot-pace";
import {
  botPose,
  mixPose,
  sitting,
  played,
  flapping,
  keep,
  type BotPose,
  type BotFlap,
} from "./bot-pose";
import { botFace, updateFace, signs } from "./bot-face";
import { botLimbs, stance } from "./bot-stance";
import { pick, type Act } from "./bot-acts";
export interface MotionSpec {
  radius: number;
  spin?: number;
  limbs?: { pixel: number } | null;
  crossesEyes?: boolean;
  keeps?: unknown;
  decor?: readonly { kind: string; fins?: readonly { hand: number }[] }[];
}
/** Direct BotMotion.swift state machine. set/play record events even when a host skips drawing.
 * Its pose, face, stance and acts are worked out in bot-pose, bot-face,
 * bot-stance and bot-acts, which read and write the fields left public here. */
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
  act: Act | null = null;
  private nextAct = 0;
  blinks = [-1, -1];
  nextBlink = 2;
  following = false;
  blinking = false;
  blinksOn = true;
  private botInterval: readonly [number, number] | null = null;
  greets = false;
  private greetAfter: number | null = null;
  greeting: number | null = null;
  greetSide = 1;
  pending: { kind: BotEvent; asked: number } | null = null;
  event: { kind: BotEvent; start: number } | null = null;
  beatsFrom = 0;
  heldSide = 1;
  private settled = false;
  random: SplitMix;
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
        const chosen = pick(this, now);
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
    signs(this, now, arrived, dozing);
    this.pose = arrived
      ? sitting(this, age, progress, swell, dozing)
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
    if (!this.limbed) played(this, now);
    this.limbs =
      this.limbed && arrived ? stance(this, age, now, dozing) : botLimbs();
    this.flap =
      this.finned && arrived ? flapping(this, age, now) : { left: 0, right: 0 };
    this.pose.x += this.limbs.x * this.stepShare * this.morph;
    this.pose.y += this.limbs.y * this.stepShare * this.morph;
    if (arrived && (!inPlace || this.morph > 0))
      updateFace(this, age, now, dt, aim, swell, dozing);
    else this.following = this.blinking = false;
    if (arrived) keep(this, age, now, dt);
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
  get greetsOnHover() {
    return (
      this.state.hovered &&
      !isAttention(this.look) &&
      (isCalm(this.look) || this.state.badge !== null)
    );
  }
  get turnsToCard() {
    return this.state.facing !== null && isAttention(this.look);
  }
  breathes(dozing: boolean) {
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
  moving(age: number) {
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
  private get stanceBeats() {
    return (
      !this.state.holds &&
      (this.look.startsWith("tool-") ||
        this.look === "plain" ||
        (this.look === "thinking" && this.sign === null))
    );
  }
}
