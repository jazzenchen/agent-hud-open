import { test } from "node:test";
import assert from "node:assert/strict";
import {
  measureSceneDepth,
  reachFromEdge,
  wantedDepth,
} from "../src/lib/use-hud-depth.ts";

const rect = (x: number, y: number, width: number, height: number) =>
  ({
    x,
    y,
    width,
    height,
    left: x,
    top: y,
    right: x + width,
    bottom: y + height,
  }) as DOMRect;
const scene = rect(0, -900, 940, 1080);

test("the reach is measured from the HUD's own edge", () => {
  const strip = rect(300, 140, 340, 40);
  assert.equal(reachFromEdge("bottom", scene, [strip]), 40);
  assert.equal(reachFromEdge("top", scene, [strip]), 1080);
  assert.equal(reachFromEdge("left", scene, [rect(0, 0, 60, 300)]), 60);
  assert.equal(reachFromEdge("right", scene, [rect(880, 0, 60, 300)]), 60);
  // Empty parts (a collapsed card) reach nothing.
  assert.equal(reachFromEdge("bottom", scene, [rect(0, 0, 0, 500)]), 0);
});

test("an opening or open panel takes the whole canvas; at rest, whole steps", () => {
  assert.equal(wantedDepth(true, 40), null);
  assert.equal(wantedDepth(false, 40), 192);
  assert.equal(wantedDepth(false, 0), 96);
  assert.equal(wantedDepth(false, 130), 288);
});

test("measureSceneDepth skips DOM rect readbacks while the panel is opening or open", () => {
  let reads = 0;
  const readScene = () => {
    reads += 1;
    return { scene, parts: [rect(300, 140, 340, 40)] };
  };
  assert.equal(measureSceneDepth("bottom", true, readScene), null);
  assert.equal(reads, 0);
  assert.equal(measureSceneDepth("bottom", false, readScene), 192);
  assert.equal(reads, 1);
});
