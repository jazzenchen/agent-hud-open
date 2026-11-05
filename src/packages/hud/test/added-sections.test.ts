// The quota history and the plan value in the hover panel: shown only once a
// layout names them, and what their agents and hidden models leave out.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addedSectionShown,
  agentShown,
  historyWindowShown,
  withAddedSection,
} from "../src/panel/added-sections.ts";

test("an added section shows only once the layout names it and does not hide it", () => {
  assert.equal(addedSectionShown({}, "quotaHistory"), false);
  assert.equal(
    addedSectionShown({ hiddenSections: ["sessions"] }, "quotaHistory"),
    false,
    "a layout saved before it",
  );
  assert.equal(
    addedSectionShown({ sections: ["blocks", "quotaHistory"] }, "quotaHistory"),
    true,
  );
  assert.equal(
    addedSectionShown(
      { sections: ["quotaHistory"], hiddenSections: ["quotaHistory"] },
      "quotaHistory",
    ),
    false,
  );
});

test("showing one added section names every section and keeps the other hidden", () => {
  const order = ["blocks", "tokens", "sessions", "quotaHistory", "planValue"];
  const shown = withAddedSection(
    { hiddenSections: ["sessions"], countdown: false },
    order,
    "planValue",
    true,
  );
  assert.deepEqual(shown.sections, order);
  assert.deepEqual(shown.hiddenSections, ["sessions", "quotaHistory"]);
  assert.equal(shown.countdown, false, "the rest of the layout stays");
  assert.equal(addedSectionShown(shown, "planValue"), true);
  const hidden = withAddedSection(shown, order, "planValue", false);
  assert.equal(addedSectionShown(hidden, "planValue"), false);
});

test("the layout's agents and hidden models choose the windows and plans", () => {
  assert.equal(agentShown("codex", {}), true);
  assert.equal(agentShown("codex", { agents: ["claude"] }), false);
  assert.equal(
    agentShown("codex", { agents: ["claude"] }, "codex"),
    true,
    "an agent's own panel shows that agent",
  );
  const opus = "claude-model:claude-opus-4-5";
  const sonnet = "claude-model:claude-sonnet-4-5";
  const layout = { hiddenModels: [opus] };
  assert.equal(
    historyWindowShown({ vendor: "claude", models: [opus] }, layout),
    false,
    "every model spending it is hidden",
  );
  assert.equal(
    historyWindowShown({ vendor: "claude", models: [opus, sonnet] }, layout),
    true,
  );
  assert.equal(
    historyWindowShown({ vendor: "cursor", models: [] }, layout),
    true,
    "no recorded call spends it, so no model hides it",
  );
});
