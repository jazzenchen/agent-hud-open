// The small acts a bot picks at random between its looks, and how long each
// lasts.
import type { BotMotion } from "./bot-motion";
import { isCalm, isWorking } from "./bot-state";
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
export interface Act {
  kind: ActKind;
  start: number;
  length: number;
  side: number;
  gaze: readonly [number, number];
  from: number;
}
export function pick(motion: BotMotion, now: number): Act {
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
  if (motion.spin !== 0) pool.push(["turn", 3]);
  if (motion.limbed) {
    pool = pool.filter(([kind]) => kind !== "tilt" && kind !== "wiggle");
    if (motion.state.hovered)
      pool.push(["wave", 4], ["hop", 2], ["scuttle", 2]);
    else if (isWorking(motion.look))
      pool.push(["scuttle", 4], ["hop", 3], ["wave", 1]);
    else if (isCalm(motion.look))
      pool = [
        ["scuttle", 3],
        ["hop", 3],
        ["stretch", 3],
        ["wave", 2],
      ];
  }
  let roll = motion.random.unit() * pool.reduce((a, b) => a + b[1], 0),
    kind = pool[0][0];
  for (const [candidate, weight] of pool) {
    roll -= weight;
    if (roll < 0) {
      kind = candidate;
      break;
    }
  }
  if (motion.greets) {
    motion.greets = false;
    if (motion.greetsOnHover) kind = "wave";
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
      kind === "glance" ? 1.1 + motion.random.unit() * 0.7 : lengths[kind],
    angle = motion.random.unit() * 2 * Math.PI;
  return {
    kind,
    start: now,
    length,
    side: motion.random.unit() < 0.5 ? -1 : 1,
    gaze: [Math.cos(angle), Math.sin(angle) * 0.7],
    from: motion.turn,
  };
}
