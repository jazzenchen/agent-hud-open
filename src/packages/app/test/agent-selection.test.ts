// Settings' agent switches write the layout's `hud.agents`: none listed shows
// every agent, one always stays on, and all of them on lists none again so an
// agent added later shows too.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  shownAgents,
  toggledAgents,
} from "../src/settings/display/agent-selection.ts";

const all = ["claude", "codex", "grok"];

test("none listed, or none of those here now, shows every agent", () => {
  assert.deepEqual(shownAgents(all, []), all);
  assert.deepEqual(shownAgents(all, ["cursor"]), all);
  assert.deepEqual(shownAgents(all, ["codex", "cursor"]), ["codex"]);
});

test("switching one off lists the rest, and the last one stays", () => {
  assert.deepEqual(toggledAgents(all, [], "codex"), ["claude", "grok"]);
  assert.deepEqual(toggledAgents(all, ["codex"], "codex"), ["codex"]);
});

test("all switched on lists none, so a new agent shows", () => {
  assert.deepEqual(toggledAgents(all, ["claude", "grok"], "codex"), []);
});

test("a listed agent the HUD lacks now keeps its place", () => {
  assert.deepEqual(toggledAgents(all, ["cursor", "claude"], "codex"), [
    "cursor",
    "claude",
    "codex",
  ]);
});
