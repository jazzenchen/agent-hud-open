// The Tokens section's agent and model filter asks Core for a selection: one
// taken out of "all" lists the rest, and putting it back asks for all again.
import { test } from "node:test";
import assert from "node:assert/strict";
import { narrowed } from "../src/statistics/section-filter.ts";

test("a selection narrows from all and returns to all", () => {
  const all = ["a", "b", "c"];
  const fewer = narrowed(null, all, ["b"], false);
  assert.deepEqual(fewer, ["a", "c"]);
  assert.deepEqual(narrowed(fewer, all, ["a", "c"], false), []);
  assert.equal(narrowed(fewer, all, ["b"], true), null);
  assert.deepEqual(
    narrowed(["a"], all, ["b"], true),
    ["a", "b"],
    "a group shown again joins the selection",
  );
});
