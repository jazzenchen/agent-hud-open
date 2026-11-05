import { test } from "node:test";
import assert from "node:assert/strict";
import type { IslandEvent } from "@agent-hud/protocol";
import {
  dockEnds,
  endAnchor,
  endLength,
  ringPath,
} from "../src/ends/end-geometry.ts";
import {
  clearMessages,
  dropMessage,
  emptyEnds,
  fadeOldest,
  keepWaiting,
  parkCard,
  TAIL_LIMIT,
} from "../src/ends/end-queue.ts";

const screen = { width: 1600, height: 1000 };
const both = { head: 1, tail: 1 };

test("an end is a pill never shorter than it is thick, longer for more digits", () => {
  assert.ok(endLength(26, 1) >= 26);
  assert.ok(endLength(26, 12) > endLength(26, 3));
  assert.ok(endLength(26, 120) > endLength(26, 12));
});

test("on a bottom dock the head sits before the first mark, the tail after the last", () => {
  const strip = { x: 700, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos" },
    { logo: 30 },
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
    { mode: "logos" },
    { logo: 30 },
    screen,
    both,
  );
  assert.ok(ends.head.y + ends.head.height <= strip.y - 6);
  assert.ok(ends.tail.y >= strip.y + strip.height + 6);
  assert.ok(ends.head.height >= ends.head.width);
});

test("a dock against the screen's start keeps both ends after it, head first", () => {
  const strip = { x: 0, y: 960, width: 200, height: 40 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "logos" },
    { logo: 30 },
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
    { mode: "logos" },
    { logo: 30 },
    screen,
    both,
  );
  assert.ok(ends.tail.x + ends.tail.width <= strip.x);
  assert.ok(ends.head.x + ends.head.width < ends.tail.x);
});

test("a Simple dock's ends are as thick as its panel, on the panel's line", () => {
  const strip = { x: 700, y: 970, width: 200, height: 30 };
  const ends = dockEnds(
    strip,
    "bottom",
    { mode: "simple" },
    { logo: 0 },
    screen,
    both,
  );
  // The panel leaves 3 points at the parked edge.
  assert.equal(ends.thick, 27);
  assert.equal(ends.tail.y, 970);
});

test("the notch grows sideways only for the ends that show, holding them inside", () => {
  const strip = { x: 700, y: 0, width: 200, height: 32 };
  const ends = dockEnds(strip, "top", { mode: "notch" }, { logo: 0 }, screen, {
    head: 0,
    tail: 3,
  });
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
  ends = dropMessage(ends, "3");
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
