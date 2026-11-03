import { test } from "node:test";
import assert from "node:assert/strict";
import { budgetText, listPrice } from "../src/format.ts";

test("a list price is written as pricing pages write it", () => {
  assert.equal(listPrice(20, "USD", "en"), "$20");
  assert.equal(listPrice(19.99, "USD", "en"), "$19.99");
  assert.equal(listPrice(200, "USD", "zh"), "$200");
});

test("the budget names its points either way, and on track in words", () => {
  assert.equal(
    budgetText({ paceDelta: 1.4, status: "onTrack" }, "en"),
    "On budget",
  );
  assert.equal(
    budgetText({ paceDelta: -8.2, status: "behind" }, "zh"),
    "比预算少用 8 个点",
  );
});
