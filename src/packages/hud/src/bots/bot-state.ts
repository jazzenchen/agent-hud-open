/** BotState.swift: one shared representation for procedural and sprite performers. */
export type BotLook =
  | "permission"
  | "question"
  | "plan"
  | "compacting"
  | "thinking"
  | "tool-edit"
  | "tool-command"
  | "tool-read"
  | "tool-long-command"
  | "plain"
  | "no-news"
  | "idle"
  | "asleep"
  | "done";
export type BotTool = "edit" | "command" | "read" | "long-command";
export type BotEvent = "start" | "failed" | "limit-hit";
export type BotBadge = "failed" | "limited";
export type BotCard = "done" | "long-done" | "reset";
export type BotFacing = "left" | "right" | "up" | "down";
export type BotPhase = "idle" | "working" | "asking" | "done" | "opened";
export type BotSign =
  | "exclamation"
  | "question"
  | "lines"
  | "cross"
  | "hourglass"
  | "ellipsis"
  | "z";
export interface BotState {
  look: BotLook;
  subagents: boolean;
  badge: BotBadge | null;
  hovered: boolean;
  holds: boolean;
  card: BotCard;
  facing: BotFacing | null;
}
/** at uses performance.now()/1000, the same monotonic clock as all bot players. */
export interface BotCue {
  event: BotEvent;
  at: number;
}
export const BOT_EVENT_FRESHNESS = 3;
export const BOT_CARD_LONG_TURN = 600;
export const BOT_SIGNS: readonly BotSign[] = [
  "exclamation",
  "question",
  "lines",
  "cross",
  "hourglass",
  "ellipsis",
  "z",
];
export function botState(value: Partial<BotState> = {}): BotState {
  return {
    look: "idle",
    subagents: false,
    badge: null,
    hovered: false,
    holds: false,
    card: "done",
    facing: null,
    ...value,
  };
}
export const isAttention = (look: BotLook) =>
  look === "permission" || look === "question" || look === "plan";
export const isWorking = (look: BotLook) =>
  look === "compacting" ||
  look === "thinking" ||
  look.startsWith("tool-") ||
  look === "plain";
export const isCalm = (look: BotLook) =>
  look === "no-news" || look === "idle" || look === "asleep";
export const staysInPlace = (look: BotLook) => isWorking(look) || isCalm(look);
export const eventRank = (event: BotEvent) =>
  ({ start: 0, "limit-hit": 1, failed: 2 })[event];
export const badgeSign = (badge: BotBadge): BotSign =>
  badge === "failed" ? "cross" : "hourglass";
export function shownSign(
  one: BotSign | null,
  other: BotSign | null,
): BotSign | null {
  if (!one || !other) return one ?? other;
  return BOT_SIGNS.indexOf(one) <= BOT_SIGNS.indexOf(other) ? one : other;
}
export function stillSign(state: BotState): BotSign | null {
  const sign =
    state.look === "permission"
      ? "exclamation"
      : state.look === "question"
        ? "question"
        : state.look === "plan"
          ? "lines"
          : null;
  return shownSign(sign, state.badge ? badgeSign(state.badge) : null);
}
export function botPhaseState(phase: BotPhase): BotState {
  return botState(
    phase === "working"
      ? { look: "plain" }
      : phase === "asking"
        ? { look: "permission" }
        : phase === "done"
          ? { look: "done" }
          : phase === "opened"
            ? { hovered: true }
            : {},
  );
}
export function facingFromEdge(
  edge: "top" | "bottom" | "left" | "right",
): BotFacing {
  return ({ top: "down", bottom: "up", left: "right", right: "left" } as const)[
    edge
  ];
}
export function facingAim(facing: BotFacing): readonly [number, number] {
  return (
    { left: [-0.6, 0], right: [0.6, 0], up: [0, -0.6], down: [0, 0.6] } as const
  )[facing];
}
export function cardForReply(start: number | null, end: number): BotCard {
  return start !== null && end - start >= BOT_CARD_LONG_TURN
    ? "long-done"
    : "done";
}
export function sameBotState(one: BotState, other: BotState): boolean {
  return (
    one.look === other.look &&
    one.subagents === other.subagents &&
    one.badge === other.badge &&
    one.hovered === other.hovered &&
    one.holds === other.holds &&
    one.card === other.card &&
    one.facing === other.facing
  );
}
