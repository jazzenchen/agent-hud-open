import { test } from "node:test";
import assert from "node:assert/strict";
import { listPrice } from "../src/format.ts";

test("a list price is written as pricing pages write it", () => {
  assert.equal(listPrice(20, "USD", "en"), "$20");
  assert.equal(listPrice(19.99, "USD", "en"), "$19.99");
  assert.equal(listPrice(200, "USD", "zh-Hans"), "$200");
});
