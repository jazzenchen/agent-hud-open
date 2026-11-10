import { test } from "node:test";
import assert from "node:assert/strict";
import { selectedAgentKeys, toggleAgentKey } from "../src/agent-selection.ts";

test("an existing installation without a dock selection keeps all available AIs", () => {
  assert.deepEqual(selectedAgentKeys(["openai", "deepseek"]), [
    "openai",
    "deepseek",
  ]);
});
test("unchecking an AI changes only the dock list, and it can be added back", () => {
  const choices = ["openai", "deepseek", "cursor"];
  const selected = toggleAgentKey(choices, [], "deepseek");
  assert.deepEqual(selected, ["openai", "cursor"]);
  assert.deepEqual(toggleAgentKey(choices, selected, "deepseek"), [
    "openai",
    "cursor",
    "deepseek",
  ]);
  assert.equal(choices.length, 3);
});
test("the last available AI cannot be unchecked", () => {
  assert.deepEqual(
    toggleAgentKey(["openai", "deepseek"], ["openai"], "openai"),
    ["openai"],
  );
});
test("an unavailable saved selection recovers to available AIs", () => {
  assert.deepEqual(selectedAgentKeys(["openai"], ["removed-client"]), [
    "openai",
  ]);
});
