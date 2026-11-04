import { test } from "node:test";
import assert from "node:assert/strict";
import { STORY, stillStory, story } from "../src/scene/flight-plan.ts";
import { FlightQueue, type HUDFlight } from "../src/scene/flight-queue.ts";
import {
  dockRoute,
  measure,
  notchRoute,
  notchStage,
  type Cubic,
  type Point,
} from "../src/scene/flight-route.ts";

const flight = (patch: Partial<HUDFlight> = {}): HUDFlight => ({
  lane: "claude:a|codex:b",
  from: "claude",
  to: "codex",
  fromLeft: true,
  arrives: false,
  falls: false,
  ...patch,
});

test("one plane in the air; more of the same ride along as its count, others wait", () => {
  const queue = new FlightQueue();
  assert.equal(queue.add(flight()), true, "the first takes off");
  assert.equal(
    queue.add(flight()),
    true,
    "the same one again is a count on it",
  );
  assert.equal(queue.current?.count, 2);
  const back = flight({ from: "codex", to: "claude", fromLeft: false });
  assert.equal(queue.add(back), false, "the way back waits");
  assert.equal(queue.add(back), false);
  assert.equal(
    queue.add(flight({ lane: "claude:a|claude:c", to: "claude" })),
    false,
  );
  assert.equal(queue.waiting.length, 2);
  const first = queue.current!.id;
  assert.equal(
    queue.finish(first + 99),
    false,
    "only the plane in the air lands",
  );
  assert.equal(queue.finish(first), true);
  assert.deepEqual(
    [queue.current?.flight.from, queue.current?.count],
    ["codex", 2],
    "the next one takes off carrying the two that waited",
  );
  queue.finish(queue.current!.id);
  queue.finish(queue.current!.id);
  assert.equal(queue.current, null);
});

const unit = (point: Point) => {
  const size = Math.hypot(point.x, point.y);
  return { x: point.x / size, y: point.y / size };
};
/** At every join the outgoing direction carries on the incoming one. */
const assertSmooth = (route: readonly Cubic[]) => {
  for (let index = 1; index < route.length; index++) {
    const [, , c, d] = route[index - 1];
    const [a, b] = route[index];
    assert.deepEqual(d, a, "segments meet");
    const incoming = unit({ x: d.x - c.x, y: d.y - c.y });
    const outgoing = unit({ x: b.x - a.x, y: b.y - a.y });
    assert.ok(
      incoming.x * outgoing.x + incoming.y * outgoing.y > 0.999,
      `join ${index} keeps its heading`,
    );
  }
};
const points = (route: readonly Cubic[]) => {
  const measured = measure(route);
  return Array.from(
    { length: 201 },
    (_, index) => measured.at(index / 200).point,
  );
};

test("two marks 64 points apart on a right dock get a long loop out into the screen", () => {
  const start = { x: 260, y: 120 };
  const end = { x: 260, y: 184 };
  const route = dockRoute(start, end, "right", { width: 300, height: 400 });
  assert.equal(route.length, 5, "five curves round an ellipse");
  assert.deepEqual(route[0][0], start);
  assert.deepEqual(route.at(-1)![3], end);
  assertSmooth(route);
  const { length } = measure(route);
  assert.ok(length >= 250, `${length} is long enough to be seen`);
  const leftmost = Math.min(...points(route).map((point) => point.x));
  assert.ok(
    leftmost < 260 - 120,
    `it reaches ${260 - leftmost} points into the screen`,
  );
  assert.ok(
    points(route).every((point) => point.x <= 260 + 1),
    "it never goes behind the dock",
  );
});

test("the route turns evenly, with no sharp bend anywhere", () => {
  for (const [start, end] of [
    [
      { x: 260, y: 120 },
      { x: 260, y: 184 },
    ],
    [
      { x: 260, y: 120 },
      { x: 260, y: 120 },
    ],
    [
      { x: 260, y: 40 },
      { x: 260, y: 360 },
    ],
  ] as const) {
    const measured = measure(dockRoute(start, end, "right"));
    const turns: number[] = [];
    let previous = measured.at(0).angle;
    for (let index = 1; index <= 100; index++) {
      const { angle } = measured.at(index / 100);
      let turn = Math.abs(angle - previous) % 360;
      if (turn > 180) turn = 360 - turn;
      turns.push(turn);
      previous = angle;
    }
    const mean = turns.reduce((sum, turn) => sum + turn, 0) / turns.length;
    const sharpest = Math.max(...turns);
    assert.ok(sharpest < mean * 2.6, `sharpest ${sharpest} vs mean ${mean}`);
  }
});

test("marks far apart get one bow; a narrow window shrinks the loop to fit", () => {
  const far = dockRoute({ x: 20, y: 400 }, { x: 20, y: 40 }, "left", {
    width: 300,
    height: 600,
  });
  assert.equal(far.length, 3, "half an ellipse");
  assertSmooth(far);
  const deepest = Math.max(...points(far).map((point) => point.x));
  assert.ok(
    deepest > 100 && deepest < 20 + 140,
    `bows ${deepest - 20} points in`,
  );

  const narrow = { width: 140, height: 400 };
  const route = dockRoute(
    { x: 120, y: 100 },
    { x: 120, y: 160 },
    "right",
    narrow,
    16,
  );
  assertSmooth(route);
  for (const point of points(route)) {
    assert.ok(
      point.x >= 16 && point.x <= narrow.width - 16 + 0.01,
      `x ${point.x} stays inside`,
    );
    assert.ok(
      point.y >= 16 && point.y <= narrow.height - 16,
      `y ${point.y} stays inside`,
    );
  }
});

test("around the notch the robots sit in wings at its sides and the route dips under it", () => {
  const stage = notchStage({ x: 400, y: 0, width: 200, height: 32 });
  assert.equal(stage.bot, 22);
  assert.deepEqual(stage.wings.left, { x: 352, width: 84 });
  assert.deepEqual(stage.wings.right, { x: 564, width: 84 });
  // Each wing reaches under the notch past the notch's rounded corner (and
  // its own inner corner), so no background shows where they meet.
  const reach = stage.radius * 2 + stage.flare;
  assert.ok(stage.wings.left.x + stage.wings.left.width >= 400 + reach);
  assert.ok(stage.wings.right.x <= 600 - reach);
  assert.deepEqual(stage.left, { x: 380, y: 16 });
  assert.deepEqual(stage.right, { x: 620, y: 16 });
  const route = notchRoute({ x: 380, y: 46 }, { x: 620, y: 46 }, stage.dip);
  assertSmooth(route);
  const lowest = Math.max(...points(route).map((point) => point.y));
  assert.ok(lowest > 32 + 20, `${lowest} passes under the notch`);
  assert.ok(
    Math.abs(lowest - stage.dip) < 2,
    `${lowest} reaches the dip ${stage.dip}`,
  );
});

const translate = (frame: Keyframe) => {
  const [x, y] = String(frame.translate).split(" ").map(parseFloat);
  return { x, y };
};
const offsets = (frames: Keyframe[]) =>
  frames.map((frame) => frame.offset as number);
const assertOrdered = (name: string, frames: Keyframe[]) => {
  const list = offsets(frames);
  assert.deepEqual(
    list,
    [...list].sort((a, b) => a - b),
    `${name} offsets only grow`,
  );
  assert.ok(
    list.every((offset) => offset >= 0 && offset <= 1),
    `${name} offsets lie in 0…1`,
  );
};

test("the story packs, folds, flies nose first, unfolds and delivers", () => {
  const start = { x: 262, y: 120 };
  const end = { x: 262, y: 184 };
  const window = { width: 300, height: 400 };
  const route = dockRoute(start, end, "right", window);
  const told = story({
    route,
    from: { x: 284, y: 120 },
    to: { x: 284, y: 184 },
    falls: false,
  });
  assert.equal(told.takeoff, STORY.pack + STORY.fold);
  assert.equal(told.landing, told.takeoff + STORY.fly);
  assert.equal(told.length, told.landing + STORY.unfold + STORY.deliver);
  for (const [name, frames] of Object.entries({
    move: told.move,
    turn: told.turn,
    shape: told.shape,
    pop: told.pop,
    flap: told.flap,
    folds: told.folds,
    letter: told.letter,
  }))
    assertOrdered(name, frames);
  for (const frame of told.move) {
    const { x, y } = translate(frame);
    assert.ok(
      x >= 0 && x <= window.width && y >= 0 && y <= window.height,
      `${x},${y} inside`,
    );
  }
  assert.deepEqual(translate(told.move[0]), start);
  assert.deepEqual(translate(told.move.at(-1)!), end);
  assert.ok(told.move.length >= 26, "the flight is sampled finely");
  assert.equal(
    told.letter.at(-1)?.translate,
    "284px 184px",
    "the letter ends in the receiver's mark",
  );
  assert.equal(
    told.shape.at(-1)?.clipPath,
    told.shape[0].clipPath,
    "an envelope again at the end",
  );
  assert.ok(
    told.dashes.length >= 20,
    `${told.dashes.length} dashes mark the way`,
  );
  for (const dash of told.dashes) assertOrdered("dash", dash.frames);
});

test("a refused plane falls from halfway and bursts open; reduced motion only shows envelopes", () => {
  const route = dockRoute({ x: 262, y: 120 }, { x: 262, y: 184 }, "right", {
    width: 300,
    height: 400,
  });
  const fell = story({
    route,
    from: { x: 284, y: 120 },
    to: { x: 284, y: 184 },
    falls: true,
  });
  assertOrdered("move", fell.move);
  assertOrdered("letter", fell.letter);
  assert.equal(fell.length, fell.landing + STORY.fall);
  assert.equal(fell.pop.at(-1)?.opacity, 0, "fades out where it fell");
  const halfway = measure(route).at(0.5).point;
  const before = translate(fell.move.at(-4)!);
  assert.ok(
    Math.hypot(before.x - halfway.x, before.y - halfway.y) < 1,
    "falls from halfway",
  );
  assert.ok(translate(fell.move.at(-1)!).y > before.y + 40, "and drops");

  const still = stillStory(false);
  assert.deepEqual(
    still.first.map((frame) => frame.opacity),
    [0, 0, 1, 1, 0, 0],
  );
  assert.equal(
    Math.max(...still.second.map((frame) => frame.opacity as number)),
    1,
  );
  const refused = stillStory(true);
  assert.equal(
    Math.max(...refused.second.map((frame) => frame.opacity as number)),
    0,
  );
});
