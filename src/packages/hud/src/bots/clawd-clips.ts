// Direct port of Bots/Clawd/ClawdClip.swift and ClawdClips.swift. Art stays unchanged.
import type { BotSign } from "./bot-state";
export type ClawdLook =
  | "thinking"
  | "retrying"
  | "edit"
  | "command"
  | "read"
  | "long-command"
  | "compacting"
  | "permission"
  | "question"
  | "plan";
export type ClawdEvent = "start" | "failed" | "limit-hit";
export type ClawdVisit = "swaying" | "turning" | "glancing" | "dozing";
export type ClawdCard = "done" | "long-done" | "reset";
export interface ClawdFrame {
  name: string;
  index: number;
}
export interface ClawdOffset {
  x: number;
  y: number;
}
export type ClawdExit =
  { kind: "cycle-end" | "any-frame" } | { kind: "through"; frame: number };
export type ClawdPace =
  | { kind: "steady" }
  | { kind: "resting"; after: number; hold: number }
  | {
      kind: "waiting";
      after: number;
      every: number;
      slowAfter: number;
      slowEvery: number;
    }
  | {
      kind: "replaying";
      after: number;
      hold: number;
      replay: number[];
      every: number;
    };
export interface ClawdClip {
  name: string;
  sprite: string;
  intro: ClawdFrame[];
  entry?: ClawdFrame[];
  loop: number[];
  outro: ClawdFrame[];
  exit: ClawdExit;
  ticks: number;
  offset: ClawdOffset;
  sign?: BotSign;
  pace: ClawdPace;
  working: boolean;
  miniShift?: number;
}
export interface ClawdShotStep {
  frame: ClawdFrame;
  ticks: number;
  sign?: BotSign;
  offset: ClawdOffset;
}
export interface ClawdShot {
  steps: ClawdShotStep[];
}
export const CLAWD_FPS = 12;
export const clawdTicks = (seconds: number) =>
  Math.max(1, swiftRound(seconds * CLAWD_FPS));
/** Swift's .rounded(): half away from zero, including negative offsets. */
export const swiftRound = (value: number) =>
  Math.sign(value) * Math.floor(Math.abs(value) + 0.5);
export const zeroOffset: ClawdOffset = { x: 0, y: 0 };
export const standing: ClawdFrame = { name: "Walking", index: 0 };
export const sameFrame = (
  one: ClawdFrame | null | undefined,
  other: ClawdFrame | null | undefined,
) =>
  one == null || other == null
    ? one == null && other == null
    : one.name === other.name && one.index === other.index;
export function easedOffset(offset: ClawdOffset, share: number): ClawdOffset {
  const x = Math.max(0, Math.min(1, share));
  const ease = x < 0.5 ? 2 * x * x : 1 - (-2 * x + 2) ** 2 / 2;
  return { x: swiftRound(offset.x * ease), y: swiftRound(offset.y * ease) };
}
export const isClawdAttention = (look: ClawdLook) =>
  look === "permission" || look === "question" || look === "plan";
export const numbers = (first: number, last: number) =>
  Array.from({ length: last - first + 1 }, (_, at) => first + at);
const frames = (name: string, first: number, last = first): ClawdFrame[] =>
  numbers(first, last).map((index) => ({ name, index }));
const clip = (
  sprite: string,
  value: Partial<ClawdClip> & Pick<ClawdClip, "name" | "loop">,
): ClawdClip => ({
  sprite,
  intro: [],
  outro: [],
  ticks: 1,
  offset: zeroOffset,
  exit: { kind: "cycle-end" },
  pace: { kind: "steady" },
  working: false,
  ...value,
});
function shot(
  all: ClawdFrame[],
  options: {
    sign?: BotSign;
    signDuring?: [number, number];
    hold?: { frame: ClawdFrame; seconds: number };
    offset?: ClawdOffset;
    easeIn?: number;
    easeOut?: number;
  } = {},
): ClawdShot {
  const main = all[0]?.name,
    {
      sign,
      signDuring,
      hold,
      offset = zeroOffset,
      easeIn = 0,
      easeOut = 0,
    } = options;
  return {
    steps: all.map((frame, at) => {
      const signed = signDuring
        ? frame.name === main &&
          frame.index >= signDuring[0] &&
          frame.index <= signDuring[1]
        : true;
      let share = at < easeIn ? at / easeIn : 1;
      if (at >= all.length - easeOut)
        share = Math.min(share, (all.length - 1 - at) / Math.max(1, easeOut));
      return {
        frame,
        ticks:
          hold && sameFrame(hold.frame, frame) ? clawdTicks(hold.seconds) : 1,
        sign: signed ? sign : undefined,
        offset: easedOffset(offset, share),
      };
    }),
  };
}

const longRun: ClawdPace = { kind: "resting", after: 20, hold: 3 };
const longWait: ClawdPace = {
  kind: "waiting",
  after: 8,
  every: 2.6,
  slowAfter: 120,
  slowEvery: 8,
};
const thinking = clip("Thinking", {
  name: "thinking",
  intro: frames("Thinking", 4, 5),
  loop: numbers(6, 19),
  outro: [...frames("Thinking", 5), ...frames("Thinking", 4), standing],
  pace: longRun,
  working: true,
  miniShift: 13,
});
const laptop = clip("Laptop", {
  name: "laptop",
  intro: frames("Laptop", 14, 16),
  loop: numbers(17, 33),
  outro: frames("Laptop", 34, 42),
  offset: { x: -1, y: 0 },
  pace: longRun,
  working: true,
  miniShift: -14,
});
const desktop = clip("Desktop", {
  name: "desktop",
  intro: frames("Desktop", 3, 15),
  entry: frames("Desktop", 7, 15),
  loop: numbers(16, 75),
  outro: frames("Desktop", 76, 87),
  exit: { kind: "any-frame" },
  offset: { x: -3, y: 0 },
  pace: longRun,
  working: true,
});
const reading = clip("LookingAround", {
  name: "reading",
  loop: numbers(6, 15),
  ticks: 2,
  pace: longRun,
  working: true,
  miniShift: 13,
});
const meditating = clip("Meditating", {
  name: "meditating",
  intro: frames("Meditating", 6, 13),
  loop: numbers(23, 34),
  outro: frames("Meditating", 45, 54),
  exit: { kind: "through", frame: 23 },
});
const pointing = clip("Pointing", {
  name: "pointing",
  intro: frames("Pointing", 6, 18),
  loop: numbers(19, 23),
  outro: [...frames("Pointing", 18), standing],
  exit: { kind: "any-frame" },
  sign: "exclamation",
  pace: longWait,
});
const waving = clip("Waving", {
  name: "waving",
  intro: frames("Waving", 3, 6),
  loop: numbers(7, 8),
  outro: [...frames("Waving", 6), standing],
  exit: { kind: "through", frame: 8 },
  sign: "question",
  pace: longWait,
});
const lightbulb = clip("Lightbulb", {
  name: "lightbulb",
  intro: frames("Lightbulb", 4, 10),
  loop: numbers(11, 38),
  outro: frames("Lightbulb", 41, 44),
  exit: { kind: "any-frame" },
  offset: { x: 0, y: 1 },
  pace: {
    kind: "replaying",
    after: 120,
    hold: 11,
    replay: numbers(26, 29),
    every: 8,
  },
});

export const clawdClips = {
  looks: {
    thinking,
    retrying: thinking,
    edit: laptop,
    command: laptop,
    "long-command": desktop,
    read: reading,
    compacting: meditating,
    permission: pointing,
    question: waving,
    plan: lightbulb,
  } satisfies Record<ClawdLook, ClawdClip>,
  events: {
    start: shot(frames("JumpingHappy", 3, 20)),
    failed: shot(frames("Facepalm", 2, 33), {
      sign: "cross",
      signDuring: [5, 31],
    }),
    "limit-hit": shot([
      ...frames("Dizzy", 6, 21),
      ...frames("Dizzy", 6, 17),
      ...frames("Dizzy", 30, 37),
      standing,
    ]),
  } satisfies Record<ClawdEvent, ClawdShot>,
  visits: {
    swaying: shot(frames("Swaying", 4, 48)),
    turning: shot([...frames("Turning", 2, 28), standing]),
    glancing: shot(frames("LookingAroundEyesOnly", 0, 41)),
    dozing: shot(
      [
        ...frames("Meditating", 6, 23),
        ...frames("Meditating", 45, 54),
        standing,
      ],
      {
        sign: "z",
        hold: { frame: { name: "Meditating", index: 23 }, seconds: 2 },
      },
    ),
  } satisfies Record<ClawdVisit, ClawdShot>,
  greet: shot([...frames("Waving", 3, 15), ...frames("Waving", 6), standing]),
  gaze: [6, 9, 12, 15, 18, 21, 27, 30],
  cards: {
    done: shot([...frames("Excited", 2, 21), standing]),
    "long-done": shot([
      ...frames("DancingHappy", 4, 38),
      ...frames("DancingHappy", 7),
      standing,
    ]),
    reset: shot([...frames("Spark", 4, 21), ...frames("Spark", 57, 68)], {
      offset: { x: -2, y: 0 },
      easeIn: 3,
      easeOut: 4,
    }),
  } satisfies Record<ClawdCard, ClawdShot>,
  pointsAtCard: { name: "Pointing", index: 19 } as ClawdFrame,
  signShifts: { Waving: { x: 7, y: 0 }, Pointing: { x: 5, y: 0 } } as Record<
    string,
    ClawdOffset
  >,
};
