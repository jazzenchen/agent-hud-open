import { test } from "node:test";
import assert from "node:assert/strict";
import { fixedTrack, rimBox, rimCore } from "../src/rim-track.ts";

test("the rim wraps the notch's core, inside its shoulders", () => {
  assert.deepEqual(rimCore({ x: 100, y: 0, width: 560, height: 520 }, 16), {
    x: 116,
    y: 0,
    width: 528,
    height: 520,
  });
  // Shoulders wider than the frame leave no core rather than a negative one.
  assert.equal(rimCore({ x: 0, y: 0, width: 10, height: 38 }, 8).width, 0);
});

test("while the notch moves, the rim's box holds both ends of the move", () => {
  const panel = rimCore({ x: 170, y: 0, width: 560, height: 520 }, 16);
  const notch = rimCore({ x: 332, y: 0, width: 236, height: 38 }, 8);
  const box = { x: 186, y: 0, width: 528, height: 520 };
  // Closing and opening need the same box, so the canvas keeps its size.
  assert.deepEqual(rimBox(panel, notch), box);
  assert.deepEqual(rimBox(notch, panel), box);
  // An alert's wings are wider than the panel: the box takes the widest of each.
  const wings = rimCore({ x: 120, y: 0, width: 660, height: 38 }, 8);
  assert.deepEqual(rimBox(panel, wings), {
    x: 128,
    y: 0,
    width: 644,
    height: 520,
  });
});

test("a rim that does not move fills its box", () => {
  const shape = { x: 0, y: 0, width: 200, height: 38, radius: 12, flare: 8 };
  const track = fixedTrack(shape);
  assert.equal(track.get(), shape);
  // Nothing ever changes, so nothing is ever called back.
  let calls = 0;
  const stop = track.subscribe(() => calls++);
  stop();
  assert.equal(calls, 0);
});
