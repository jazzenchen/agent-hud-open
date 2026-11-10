import { test } from "node:test";
import assert from "node:assert/strict";
import { dockFontWeight, scaleDockMetrics } from "../src/dock-scale.ts";

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
test("size scales text, marks, gaps and hit centers together, including weight", () => {
  const small = scaleDockMetrics(metrics, true, 0.6);
  const large = scaleDockMetrics(metrics, true, 1.6);
  for (const key of [
    "width",
    "font",
    "logo",
    "gap",
    "labelGap",
    "bar",
  ] as const)
    assert.ok(Math.abs(large[key] / small[key] - 1.6 / 0.6) < 0.00001);
  assert.ok(dockFontWeight(0.6) < dockFontWeight(1.6));
  assert.ok(
    Math.abs(large.centers[0] / small.centers[0] - 1.6 / 0.6) < 0.00001,
  );
});
test("height preserves content and width; corner roundness is independent", () => {
  const low = scaleDockMetrics(metrics, true, 1, 0.75, 0);
  const high = scaleDockMetrics(metrics, true, 1, 1.75, 1);
  assert.equal(low.width, high.width);
  assert.equal(low.logo, high.logo);
  assert.equal(low.font, high.font);
  assert.ok(low.height - 3 >= low.mark);
  assert.ok(low.padding.top >= 0);
  assert.equal(low.radius, 0);
  assert.ok(high.radius > 0);
  assert.ok(high.height > low.height);
});
