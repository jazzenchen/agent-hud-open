import { test } from "node:test";
import assert from "node:assert/strict";
import type { StripMetrics } from "../src/geometry.ts";
import {
  drawerHandleInset,
  drawerHandleRect,
} from "../src/scene/drawer-geometry.ts";

const metrics = (mark: number) => ({ mark }) as StripMetrics;

test("the bar floats clear of the screen edge, as a phone's does", () => {
  const strip = { x: 8, y: 400, width: 40, height: 200 };
  const bar = drawerHandleRect("left", strip, metrics(22));
  // One mark thick, a little way in from the edge the dock hangs from.
  assert.equal(bar.x, strip.x + drawerHandleInset(metrics(22)));
  assert.ok(drawerHandleInset(metrics(22)) > 0);
  assert.equal(bar.width, 6);
  // As long as its marks ran, and centred on where they were.
  assert.equal(bar.height, 75);
  assert.equal(bar.y, 400 + (200 - 75) / 2);
});

test("a right-hand dock folds to a bar inside the screen's right edge", () => {
  const strip = { x: 1500, y: 400, width: 40, height: 200 };
  const bar = drawerHandleRect("right", strip, metrics(22));
  assert.equal(
    bar.x + bar.width,
    strip.x + strip.width - drawerHandleInset(metrics(22)),
  );
  assert.equal(bar.y, 400 + (200 - 75) / 2);
});

test("a dock on the top edge folds to a bar along it, just below", () => {
  const strip = { x: 700, y: 0, width: 200, height: 40 };
  const bar = drawerHandleRect("top", strip, metrics(22));
  assert.equal(bar.x, 700 + (200 - 75) / 2);
  assert.equal(bar.width, 75);
  assert.equal(bar.y, drawerHandleInset(metrics(22)));
  assert.equal(bar.height, 6);
});

test("a bottom dock folds to a bar just above the screen's bottom edge", () => {
  const strip = { x: 700, y: 960, width: 200, height: 40 };
  const bar = drawerHandleRect("bottom", strip, metrics(22));
  assert.equal(
    bar.y + bar.height,
    strip.y + strip.height - drawerHandleInset(metrics(22)),
  );
  assert.equal(bar.x, 700 + (200 - 75) / 2);
});

test("a bar stays inside a dock too short for it, and thick enough to see", () => {
  const strip = { x: 8, y: 400, width: 32, height: 32 };
  const bar = drawerHandleRect("left", strip, metrics(22));
  assert.equal(bar.height, 32);
  assert.equal(bar.width, 6);
  // A dock's marks at their smallest still leave a bar a pointer can find.
  assert.equal(drawerHandleRect("left", strip, metrics(1)).width, 4);
  assert.equal(drawerHandleInset(metrics(1)), 5);
});
