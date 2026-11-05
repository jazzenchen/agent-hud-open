import { test } from "node:test";
import assert from "node:assert/strict";
import type { IslandEvent } from "@agent-hud/protocol";
import {
  dockEnds,
  endAnchor,
  endBadge,
  ringPath,
} from "../src/ends/end-geometry.ts";
import {
  clearMessages,
  dropMessages,
  mergeEnds,
  emptyEnds,
  fadeOldest,
  keepWaiting,
  parkCard,
  TAIL_LIMIT,
} from "../src/ends/end-queue.ts";

const screen = { width: 1600, height: 1000 };
const logos = {
  logo: 30,
  mark: 30,
  padding: { top: 0, right: 0, bottom: 0, left: 0 },
};
const both = { head: 1, tail: 1 };

test("an end is a circle as thick as the dock, its number in a badge on the edge", () => {
  const strip = { x: 700, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos", simpleSize: 26 },
    logos,
    screen,
    {
      head: 3,
      tail: 12,
    },
  );
  assert.equal(ends.head.width, ends.head.height);
  assert.equal(ends.tail.width, ends.thick);
  assert.equal(endBadge(26, 1).label, "1");
  assert.equal(endBadge(26, 120).label, "99+");
  // The badge sits on the circle's top-right edge.
  assert.ok(endBadge(26, 1).x > 13 && endBadge(26, 1).y < 13);
});

test("on a bottom dock the head sits before the first mark, the tail after the last", () => {
  const strip = { x: 700, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos", simpleSize: 26 },
    logos,
    screen,
    both,
  );
  assert.ok(ends.head.x + ends.head.width <= strip.x - 6);
  assert.ok(ends.tail.x >= strip.x + strip.width + 6);
  // Centred across the dock.
  assert.equal(ends.head.y + ends.head.height / 2, strip.y + strip.height / 2);
  assert.deepEqual(ends.wings, { head: 0, tail: 0 });
});

test("on a side dock the head is above and the tail below, standing upright", () => {
  const strip = { x: 1560, y: 400, width: 40, height: 200 };
  const ends = dockEnds(
    strip,
    "right",
    { mode: "logos", simpleSize: 26 },
    logos,
    screen,
    both,
  );
  assert.ok(ends.head.y + ends.head.height <= strip.y - 6);
  assert.ok(ends.tail.y >= strip.y + strip.height + 6);
  assert.equal(ends.head.height, ends.head.width);
});

test("a dock against the screen's start keeps both ends after it, head first", () => {
  const strip = { x: 0, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos", simpleSize: 26 },
    logos,
    screen,
    both,
  );
  assert.ok(ends.head.x >= strip.width);
  assert.ok(ends.tail.x > ends.head.x + ends.head.width);
});

test("a dock against the screen's end keeps both ends before it, head first", () => {
  const strip = { x: 1400, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos", simpleSize: 26 },
    logos,
    screen,
    both,
  );
  assert.ok(ends.tail.x + ends.tail.width <= strip.x);
  assert.ok(ends.head.x + ends.head.width < ends.tail.x);
});

test("a Simple dock's ends are as thick as its row of rings, on its ring column", () => {
  const rings = {
    logo: 14,
    mark: 26,
    padding: { top: 5, right: 12, bottom: 5, left: 6 },
  };
  const config = { mode: "simple" as const, simpleSize: 26 };
  const bottom = { x: 700, y: 961, width: 200, height: 39 };
  const ends = dockEnds(bottom, "bottom", config, rings, screen, both);
  assert.equal(ends.thick, 36);
  assert.equal(ends.tail.y, 961);
  // A side edge's panel is wider with its numbers; the circle stays the same.
  const right = { x: 1520, y: 400, width: 80, height: 200 };
  const side = dockEnds(right, "right", config, rings, screen, both);
  assert.equal(side.thick, 36);
  assert.equal(side.head.x + side.head.width / 2, 1520 + 6 + 13);
});

test("the notch grows sideways only for the ends that show, holding them inside", () => {
  const strip = { x: 700, y: 0, width: 200, height: 32 };
  const ends = dockEnds(
    strip,
    "top",
    { mode: "notch", simpleSize: 26 },
    logos,
    screen,
    {
      head: 0,
      tail: 3,
    },
  );
  assert.equal(ends.wings.head, 0);
  assert.ok(ends.wings.tail > ends.tail.width);
  assert.ok(ends.tail.x > strip.x + strip.width);
  assert.ok(
    ends.tail.x + ends.tail.width < strip.x + strip.width + ends.wings.tail,
  );
  assert.ok(ends.tail.y > 0 && ends.tail.y + ends.tail.height < 32);
});

test("a card from an end centres on it along the dock", () => {
  const rect = { x: 100, y: 960, width: 40, height: 26 };
  assert.equal(endAnchor(rect, "bottom"), 120);
  assert.equal(endAnchor(rect, "left"), 973);
});

test("the fade ring runs the pill's whole outline", () => {
  const { d, length } = ringPath(40, 20, 0);
  assert.ok(d.startsWith("M20 0"));
  // Two straight sides of 20 and a full circle of radius 10.
  assert.ok(Math.abs(length - (40 + 2 * Math.PI * 10)) < 1e-9);
});

const message = (id: string): IslandEvent => ({
  id,
  kind: "completion",
  at: "2026-11-05T10:00:00Z",
  source: "claude",
});

test("a request parks once in the head, in the order it parked", () => {
  let ends = parkCard(emptyEnds, { type: "request", id: "a" });
  ends = parkCard(ends, { type: "request", id: "b" });
  ends = parkCard(ends, { type: "request", id: "a" });
  assert.deepEqual(ends.head, ["a", "b"]);
});

test("the tail keeps messages newest first and lets the oldest go", () => {
  let ends = emptyEnds;
  for (const id of ["1", "2", "3"])
    ends = parkCard(ends, { type: "news", id, at: "", event: message(id) });
  assert.deepEqual(
    ends.tail.map((item) => item.id),
    ["3", "2", "1"],
  );
  ends = fadeOldest(ends);
  assert.deepEqual(
    ends.tail.map((item) => item.id),
    ["3", "2"],
  );
  ends = dropMessages(ends, ["3"]);
  assert.deepEqual(
    ends.tail.map((item) => item.id),
    ["2"],
  );
  assert.deepEqual(clearMessages(ends).tail, []);
  assert.equal(fadeOldest(emptyEnds), emptyEnds);
});

test("the tail holds at most its limit, dropping the oldest", () => {
  let ends = emptyEnds;
  for (let i = 0; i < TAIL_LIMIT + 5; i++)
    ends = parkCard(ends, {
      type: "news",
      id: String(i),
      at: "",
      event: message(String(i)),
    });
  assert.equal(ends.tail.length, TAIL_LIMIT);
  assert.equal(ends.tail[0].id, String(TAIL_LIMIT + 4));
});

test("a request answered anywhere leaves the head", () => {
  const ends = { head: ["a", "b"], tail: [] };
  assert.deepEqual(keepWaiting(ends, ["b"]).head, ["b"]);
  assert.equal(keepWaiting(ends, ["a", "b", "c"]), ends);
});

test("a screen that starts late takes what the others hold, once each", () => {
  const mine = {
    head: ["a"],
    tail: [{ id: "2", at: "2026-11-05T10:00:02Z", event: message("2") }],
  };
  const theirs = {
    head: ["a", "b"],
    tail: [
      { id: "3", at: "2026-11-05T10:00:03Z", event: message("3") },
      { id: "2", at: "2026-11-05T10:00:02Z", event: message("2") },
      { id: "1", at: "2026-11-05T10:00:01Z", event: message("1") },
    ],
  };
  const merged = mergeEnds(mine, theirs);
  assert.deepEqual(merged.head, ["a", "b"]);
  assert.deepEqual(
    merged.tail.map((item) => item.id),
    ["3", "2", "1"],
  );
  assert.equal(mergeEnds(merged, theirs), merged);
});
