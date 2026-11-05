// The quota history draws one chart per agent: its windows together, an
// account apart only where the agent has several.
import { test } from "node:test";
import assert from "node:assert/strict";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import {
  quotaGroups,
  readingsAt,
  windowColor,
} from "../src/panel/quota-groups.ts";

const window = (
  id: string,
  vendor: string,
  accountId: string,
  segments: [number, number][][] = [],
): QuotaHistoryWindow => ({
  id,
  vendor,
  accountId,
  account: `${accountId}@example.com`,
  name: id,
  windowDurationSeconds: null,
  resetAt: null,
  segments,
  resets: [],
});

test("an agent's windows share one chart, its accounts apart only when it has several", () => {
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

test("each window's reading near a time, none across a gap", () => {
  const readings = readingsAt(
    [
      window("a", "claude", "a", [
        [
          [0, 10],
          [100, 20],
        ],
        [[1_000, 50]],
      ]),
      window("b", "claude", "a", [[[600, 5]]]),
    ],
    110,
    50,
  );
  assert.deepEqual(readings, [[100, 20], null]);
  assert.notEqual(windowColor(0, false), windowColor(1, false));
});
