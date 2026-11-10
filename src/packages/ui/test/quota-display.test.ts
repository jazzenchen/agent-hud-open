import { test } from "node:test";
import assert from "node:assert/strict";
import {
  quotaPercent,
  quotaPercentLabel,
  quotaDisplayLabel,
} from "../src/format.ts";

test("quota display defaults to remaining and reverses only known percentages", () => {
  for (const [used, remaining] of [
    [0, 100],
    [40, 60],
    [92, 8],
    [100, 0],
  ]) {
    assert.equal(quotaPercent(used), remaining);
    assert.equal(quotaPercent(used, "consumed"), used);
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
  assert.equal(quotaPercentLabel(40.4), "60%");
});

test("quota display labels describe the selected direction in each language", () => {
  assert.equal(quotaDisplayLabel(), "Quota remaining");
  assert.equal(quotaDisplayLabel("remaining"), "Quota remaining");
  assert.equal(quotaDisplayLabel("consumed", "zh"), "已用额度");
  assert.equal(quotaDisplayLabel("remaining", "zh"), "剩余额度");
});
