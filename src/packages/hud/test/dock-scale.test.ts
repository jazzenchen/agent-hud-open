import { test } from "node:test";
import assert from "node:assert/strict";
import { scaleDockMetrics } from "../src/dock-scale.ts";

const metrics = {
  width: 100,
  height: 30,
  gap: 10,
  logo: 20,
  mark: 24,
  font: 11,
  labelGap: 6,
  label: 20,
  bar: 30,
  radius: 12,
  padding: { top: 3, right: 10, bottom: 3, left: 10 },
  centers: [22, 66],
};
test("dock drawings and pointer centers shrink together, retaining the edge margin", () => {
  const scaled = scaleDockMetrics(metrics, true);
  assert.equal(scaled.width, 92);
  assert.equal(scaled.height, 30.6);
  assert.ok(Math.abs(scaled.logo - 18.4) < 0.00001);
  assert.ok(Math.abs(scaled.centers[0] - 20.24) < 0.00001);
  assert.equal(metrics.width, 100);
});
test("a vertical dock retains the transparent margin on its horizontal edge", () => {
  const scaled = scaleDockMetrics(metrics, false);
  assert.equal(scaled.width, 95);
  assert.equal(scaled.height, 27.6);
});
