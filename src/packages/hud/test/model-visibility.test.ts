import test from "node:test";
import assert from "node:assert/strict";
import {
  orderedModels,
  visibleModelCounts,
} from "../src/panel/model-visibility.ts";

test("model preferences hide individually, retain order and allow new models", () => {
  const models = [{ id: "codex:a" }, { id: "codex:b" }, { id: "codex:new" }];
  const visible = orderedModels(
    models,
    ["codex:away", "codex:b", "codex:a"],
    ["codex:a"],
  );
  assert.deepEqual(
    visible.map((model) => model.id),
    ["codex:b", "codex:new"],
  );
  assert.deepEqual(
    visibleModelCounts({ "codex:a": 10, "codex:b": 20, "codex:new": 30 }, 60, [
      "codex:a",
    ]),
    {
      counts: { "codex:b": 20, "codex:new": 30 },
      total: 50,
    },
  );
  assert.deepEqual(
    visibleModelCounts(
      { "codex:a": 10 },
      10,
      models.map((m) => m.id),
    ),
    { counts: {}, total: 0 },
  );
  assert.deepEqual(visibleModelCounts({ "codex:a": 10 }, 13, ["codex:a"]), {
    counts: {},
    total: 3,
  });
  assert.deepEqual(
    orderedModels(
      [...models, { id: "codex:away" }],
      ["codex:away", "codex:b"],
    ).map((model) => model.id),
    ["codex:away", "codex:b", "codex:a", "codex:new"],
  );
});
