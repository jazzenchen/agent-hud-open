import {
  BOT_EVENT_FRESHNESS,
  botState,
  eventRank,
  isAttention,
  isCalm,
  type BotCue,
  type BotEvent,
  type BotSign,
  type BotState,
} from "./bot-state";
import { ClawdPlayer, type ClawdPlayerOptions } from "./clawd-player";
import { canonicalClawdFrame, type ClawdSheet } from "./clawd";
import type { ClawdLook } from "./clawd-clips";

const onGrid = (time: number) => Math.ceil((time - 0.004) * 12) / 12;
const following = (time: number) => (Math.floor((time + 0.004) * 12) + 1) / 12;
const mask64 = (1n << 64n) - 1n;
function splitMix(seed: bigint) {
  let state = seed;
  return () => {
    state = (state + 0x9e3779b97f4a7c15n) & mask64;
    let z = state;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & mask64;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & mask64;
    z ^= z >> 31n;
    return Number(z >> 11n) / 9007199254740992;
  };
}
export interface ClawdPerformerOptions extends ClawdPlayerOptions {
  sheet?: ClawdSheet;
  staysBot?: boolean;
  arrivesAfter?: number;
  seed?: number | bigint;
  random?: () => number;
}

/** Full ClawdPerformer.swift: presentation inputs never restart a clip already acting out that look. */
export class ClawdPerformer {
  readonly player: ClawdPlayer;
  readonly staysBot: boolean;
  readonly arrivesAfter: number;
  state: BotState = botState();
  due = Infinity;
  private told = false;
  private arrival: number | null = null;
  private onStage: boolean;
  private nextVisit: number | null = null;
  private visiting = false;
  private greetAt: number | null = null;
  private watching = false;
  private asked: { event: BotEvent; at: number } | null = null;
  private lastCue: BotCue | null = null;
  private readonly random: () => number;

  constructor(options: ClawdPerformerOptions = {}) {
    this.staysBot = options.staysBot ?? false;
    this.arrivesAfter = this.staysBot
      ? Math.max(0, options.arrivesAfter ?? 0)
      : 0;
    this.onStage = !this.staysBot;
    const canonical =
      options.canonical ??
      (options.sheet
        ? (frame) => canonicalClawdFrame(options.sheet!, frame)
        : undefined);
    this.player = new ClawdPlayer({
      clips: options.clips,
      canonical,
      rest: this.staysBot ? "standing" : "logo",
    });
    this.random =
      options.random ??
      splitMix(
        BigInt(
          options.seed ?? Math.floor(Math.random() * Number.MAX_SAFE_INTEGER),
        ),
      );
  }
  get picture() {
    return this.onStage ? this.player.picture : null;
  }
  get logoSign(): BotSign | undefined {
    return this.state.badge === "failed"
      ? "cross"
      : this.state.badge === "limited"
        ? "hourglass"
        : undefined;
  }
  get appearanceChangedAt() {
    return this.player.appearanceChangedAt;
  }
  get playing() {
    return this.player.playing;
  }
  get switchPoint() {
    return this.player.switchPoint;
  }
  get atSwitchPoint() {
    return this.player.atSwitchPoint;
  }

  set(next: BotState, now: number) {
    const before = this.told ? this.state : null;
    this.state = { ...next };
    this.told = true;
    if (this.staysBot && this.arrival === null)
      this.arrival = onGrid(now + this.arrivesAfter);
    if (this.onStage) this.apply(before, now);
    this.findDue(now);
  }
  resume(next: BotState, now: number) {
    this.set(next, now);
    const look = ClawdPerformer.lookFor(next);
    if (
      this.onStage &&
      (next.facing === null || !isAttention(next.look)) &&
      look !== null
    )
      this.player.resume(look, now, next.subagents);
    this.findDue(now);
  }
  play(event: BotEvent, now: number) {
    this.letGoOfStaleEvent(now);
    if (
      this.asked &&
      (this.player.pendingEvent !== null || !this.onStage) &&
      eventRank(this.asked.event) > eventRank(event)
    )
      return;
    this.asked = { event, at: now };
    if (this.onStage) this.player.play(event, now);
    this.findDue(now);
  }
  /** A cue survives React snapshots without replaying, and is discarded if its original timestamp is stale. */
  playCue(cue: BotCue | null | undefined, now: number) {
    if (!cue || now - cue.at > BOT_EVENT_FRESHNESS) return;
    if (
      this.lastCue &&
      (cue.at < this.lastCue.at ||
        (cue.at === this.lastCue.at &&
          eventRank(cue.event) <= eventRank(this.lastCue.event)))
    )
      return;
    this.lastCue = { ...cue };
    this.play(cue.event, now);
    if (this.asked?.event === cue.event) this.asked.at = cue.at;
  }
  step(now: number, aim: readonly [number, number] = [0, 0]) {
    if (!this.onStage && this.arrival !== null && now >= this.arrival - 0.004) {
      this.onStage = true;
      if (this.asked && this.arrival - this.asked.at <= BOT_EVENT_FRESHNESS)
        this.player.play(this.asked.event, this.arrival);
      this.apply(null, this.arrival);
    }
    this.letGoOfStaleEvent(now);
    if (this.greetAt !== null && now >= this.greetAt - 0.004) {
      this.greetAt = null;
      if (isCalm(this.state.look)) {
        this.player.greet(now);
        this.watching = true;
      }
    }
    if (this.watching) this.player.look(aim, now);
    if (
      this.visits &&
      this.nextVisit !== null &&
      now >= this.nextVisit - 0.004
    ) {
      const idle = ["swaying", "turning", "glancing"] as const;
      this.player.visit(
        this.state.look === "asleep"
          ? "dozing"
          : idle[Math.floor(this.random() * idle.length)],
        now,
      );
      this.visiting = this.player.picture !== null;
      this.nextVisit = null;
      if (!this.visiting) this.scheduleVisit(now);
    }
    this.player.advance(now);
    if (this.visiting && this.player.picture === null) {
      this.visiting = false;
      this.scheduleVisit(now);
    }
    this.findDue(now);
    return this.picture;
  }
  private apply(before: BotState | null, now: number) {
    this.letGoOfStaleEvent(now);
    const state = this.state,
      look = ClawdPerformer.lookFor(state);
    if (state.look === "done") {
      if (before?.look !== "done" || before.card !== state.card)
        this.player.card(state.card, now);
    } else if (
      isAttention(state.look) &&
      state.facing !== null &&
      look !== null
    ) {
      if (before?.facing !== state.facing || before.look !== state.look) {
        if (state.facing === "left" || state.facing === "right")
          this.player.pointAtCard(state.facing === "left", now);
        else this.player.standAtCard(look, now);
      }
    } else this.player.show(look, now, state.subagents);
    this.player.setBadge(this.logoSign, now);
    if (state.hovered !== (before?.hovered ?? false)) {
      if (state.hovered) this.greetAt = onGrid(now + 0.3);
      else {
        this.greetAt = null;
        this.watching = false;
        this.player.endHover(now);
      }
    }
    if (!isCalm(state.look)) this.watching = false;
    if (state.look !== before?.look || state.hovered !== before?.hovered) {
      this.visiting = false;
      this.nextVisit = null;
      if (this.visits) this.scheduleVisit(now);
    }
  }
  private get visits() {
    return (
      !this.staysBot &&
      !this.state.hovered &&
      (this.state.look === "idle" || this.state.look === "asleep")
    );
  }
  private scheduleVisit(now: number) {
    this.nextVisit = onGrid(
      now +
        (this.state.look === "asleep"
          ? 180 + this.random() * 120
          : 20 + this.random() * 15),
    );
  }
  private letGoOfStaleEvent(now: number) {
    if (!this.asked || now - this.asked.at <= BOT_EVENT_FRESHNESS) return;
    if (this.onStage && this.player.pendingEvent !== null)
      this.player.cancelEvent();
    this.asked = null;
  }
  private findDue(now: number) {
    this.due = Math.min(
      this.onStage ? this.player.due : (this.arrival ?? Infinity),
      this.greetAt ?? Infinity,
      this.visits ? (this.nextVisit ?? Infinity) : Infinity,
      this.watching ? following(now) : Infinity,
    );
  }
  static lookFor(state: BotState): ClawdLook | null {
    switch (state.look) {
      case "permission":
      case "question":
      case "plan":
      case "compacting":
        return state.look;
      case "thinking":
        return state.holds ? "retrying" : "thinking";
      case "tool-edit":
        return "edit";
      case "tool-command":
      case "plain":
        return "command";
      case "tool-read":
        return "read";
      case "tool-long-command":
        return "long-command";
      default:
        return null;
    }
  }
}
