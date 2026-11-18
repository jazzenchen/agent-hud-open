import { test } from "node:test";
import assert from "node:assert/strict";
import { noSessionsMatch } from "../src/panel/session-filters.ts";
import {
  noAnswer,
  settled,
  shownAnswer,
  type PanelAnswer,
} from "../src/panel-answers.ts";

test("an agent's own panel without sessions has none yet; only the layout's filters make none match", () => {
  // Antigravity's own panel, no Antigravity session: its agent is no filter.
  assert.equal(noSessionsMatch({}, true, 0, 0), false);
  // The layout's agents do not apply in an agent's own panel either.
  assert.equal(noSessionsMatch({ agents: ["codex"] }, true, 0, 0), false);
  assert.equal(noSessionsMatch({ agents: ["codex"] }, false, 0, 3), true);
  assert.equal(
    noSessionsMatch({ projects: ["agent-hud"] }, true, 0, 2),
    true,
    "its sessions are all in other projects",
  );
  assert.equal(
    noSessionsMatch({ projects: ["agent-hud"] }, true, 0, 0),
    false,
    "no session to leave out",
  );
  assert.equal(
    noSessionsMatch({ projects: ["agent-hud"] }, false, 1, 4),
    false,
    "a row is listed",
  );
  assert.equal(noSessionsMatch({}, false, 0, 4), false);
});

test("a section keeps the last answer to its query while the next revision's is asked", () => {
  const base = JSON.stringify(["statistics", { providers: ["antigravity"] }]);
  const key = (revision: number) =>
    JSON.stringify(["statistics", revision, { providers: ["antigravity"] }]);
  const answer = (
    revision: number,
    order: number,
    tokens: number | null,
  ): PanelAnswer<number> => ({
    key: key(revision),
    base,
    order,
    data: tokens,
    error: null,
  });
  assert.deepEqual(
    shownAnswer(noAnswer, key(1), base),
    { data: null, error: null },
    "nothing before the first answer",
  );
  let kept: PanelAnswer<number> = noAnswer;
  kept = settled(kept, answer(1, 1, 500));
  assert.equal(shownAnswer(kept, key(1), base).data, 500);
  // Core moved on twice before revision 2's answer came: revision 1's stays.
  assert.equal(shownAnswer(kept, key(3), base).data, 500);
  // Revision 2's answer comes after revision 3 was asked: it still counts.
  kept = settled(kept, answer(2, 2, 520));
  assert.equal(shownAnswer(kept, key(3), base).data, 520);
  kept = settled(kept, answer(3, 3, 530));
  // A late answer to a query asked earlier does not replace a newer one.
  kept = settled(kept, answer(2, 2, 520));
  assert.equal(shownAnswer(kept, key(3), base).data, 530);
  // Other parameters never show this answer.
  const codex = { providers: ["codex"] };
  assert.equal(
    shownAnswer(
      kept,
      JSON.stringify(["statistics", 3, codex]),
      JSON.stringify(["statistics", codex]),
    ).data,
    null,
  );
  // An error belongs to its own query only.
  kept = settled(kept, {
    ...answer(4, 4, null),
    error: "Statistics could not be read",
  });
  assert.deepEqual(shownAnswer(kept, key(4), base), {
    data: null,
    error: "Statistics could not be read",
  });
  assert.equal(shownAnswer(kept, key(5), base).error, null);
});

test("two panels asking the same question show the same answer, whichever asked the query that answered", () => {
  const base = JSON.stringify(["plan_value", { hiddenModels: [] }]);
  const key = (revision: number) =>
    JSON.stringify(["plan_value", revision, { hiddenModels: [] }]);
  // The visible panel asked at revision 1 and has its answer; the measuring
  // copy asked only at revision 2, which Core has not answered yet (Open #116).
  const visible: PanelAnswer<string> = {
    key: key(1),
    base,
    order: 1,
    data: "three plans",
    error: null,
  };
  const last = "three plans";
  assert.equal(shownAnswer(visible, key(2), base, last).data, "three plans");
  assert.equal(
    shownAnswer(noAnswer, key(2), base, last).data,
    "three plans",
    "the copy measures the rows the visible panel shows, not Loading",
  );
  // A panel's own answer to its query comes first, an error included.
  assert.equal(
    shownAnswer({ ...visible, key: key(2) }, key(2), base, "older").data,
    "three plans",
  );
  assert.deepEqual(
    shownAnswer(
      { ...visible, key: key(2), data: null, error: "Plan value failed" },
      key(2),
      base,
      last,
    ),
    { data: null, error: "Plan value failed" },
  );
  // With no last answer to the question, a panel keeps its own.
  assert.equal(shownAnswer(visible, key(2), base).data, "three plans");
});
