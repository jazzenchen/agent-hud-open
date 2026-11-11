import { test } from "node:test";
import assert from "node:assert/strict";
import { quotaPercent, quotaPercentLabel } from "../src/format.ts";

test("quota display defaults to consumed and reverses only known percentages", () => {
  for (const [used, remaining] of [
    [0, 100],
    [40, 60],
    [92, 8],
    [100, 0],
  ]) {
    assert.equal(quotaPercent(used), used);
    assert.equal(quotaPercent(used, "remaining"), remaining);
    assert.equal(quotaPercentLabel(used, "remaining"), `${remaining}%`);
  }
  for (const unknown of [null, undefined, NaN, Infinity]) {
    assert.equal(quotaPercent(unknown, "remaining"), null);
    assert.equal(quotaPercentLabel(unknown, "remaining"), "—");
  }
  assert.equal(quotaPercent(-10, "remaining"), 100);
  assert.equal(quotaPercent(110, "remaining"), 0);
  assert.equal(quotaPercentLabel(40.4, "remaining"), "60%");
  assert.equal(quotaPercentLabel(40.4), "40%");
});
