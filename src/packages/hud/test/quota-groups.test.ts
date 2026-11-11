// The quota history draws one block per agent, an account apart only where
// the agent has several, and one bar per cycle of each window.
import { test } from "node:test";
import assert from "node:assert/strict";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import { cycleLevel, quotaGroups } from "../src/panel/quota-groups.ts";

const window = (
  id: string,
  vendor: string,
  accountId: string,
): QuotaHistoryWindow => ({
  id,
  vendor,
  accountId,
  account: `${accountId}@example.com`,
  name: id,
  windowDurationSeconds: null,
  resetAt: null,
  segments: [],
  resets: [],
});

test("an agent's windows share one block, its accounts apart only when it has several", () => {
  const groups = quotaGroups([
    window("c5", "claude", "a"),
    window("x1", "codex", "x"),
    window("cw", "claude", "a"),
    window("x2", "codex", "y"),
  ]);
  assert.deepEqual(
    groups.map((group) => [
      group.key,
      group.account,
      group.windows.map((w) => w.id),
    ]),
    [
      ["claude", null, ["c5", "cw"]],
      ["codex|x", "x@example.com", ["x1"]],
      ["codex|y", "y@example.com", ["x2"]],
    ],
  );
});

test("a cycle warns from 70% used and is critical from 90%", () => {
  assert.deepEqual(
    [cycleLevel(10), cycleLevel(70), cycleLevel(95)],
    ["normal", "warning", "critical"],
  );
});
