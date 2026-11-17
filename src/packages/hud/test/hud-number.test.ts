import { test } from "node:test";
import assert from "node:assert/strict";
import type { PanelLayout, Quota } from "@agent-hud/protocol";
import { windowReading } from "../src/held-reading.ts";
import { shownRows } from "../src/quota-rows.ts";

const now = Date.parse("2026-11-18T12:00:00Z");
const row = (window: string, usedPercent: number, status: string) =>
  ({
    id: `acct:${window}`,
    vendor: "antigravity",
    name: window,
    usedPercent,
    resetAt: "2026-11-18T17:00:00Z",
    status,
    account: "a@example.com",
    accountId: "acct",
  }) as Quota;
const quotas = [
  row("claude-5h", 100, "critical"),
  row("gemini-5h", 20, "normal"),
];
// What configuredVendors sets on a mark (model.ts `withShownWindows`).
const reading = (blocks: PanelLayout["blocks"]) => {
  const { used, heldUsed, level } = windowReading(
    shownRows(quotas, { blocks } as PanelLayout),
    now,
  );
  return [used, heldUsed, level];
};

test("the HUD number leaves out the windows the panel hides (Open #91)", () => {
  assert.deepEqual(reading({}), [100, null, "critical"]);
  assert.deepEqual(
    reading({ antigravity: { hiddenRows: ["acct:claude-5h"] } }),
    [20, null, "normal"],
  );
  assert.deepEqual(reading({ antigravity: { hiddenAccounts: ["acct"] } }), [
    null,
    null,
    "stale",
  ]);
});
