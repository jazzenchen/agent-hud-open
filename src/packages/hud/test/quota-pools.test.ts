import { visibleQuotaPools } from "../src/quota-rows.ts";
import test from "node:test";
import assert from "node:assert/strict";
import { quotaPercent, quotaPercentLabel } from "@agent-hud/ui/format";
import type { QuotaPool } from "@agent-hud/protocol";

test("pooled quota percentages and member shares honor consumed vs remaining modes and clamp 0..100", () => {
  const pool: QuotaPool = {
    id: "pool:antigravity:carbon-5h",
    vendor: "antigravity",
    windowKey: "antigravity:carbon-5h",
    name: "Gemini Carbon · 5h",
    shortLabel: "Carbon 5h",
    usedPercent: 60,
    status: "normal",
    nextResetAt: "2026-04-12T15:00:00Z",
    nextResetGainPercent: 50,
    recommendedAccountId: "acct-2",
    members: [
      {
        quotaId: "q1",
        accountId: "acct-1",
        accountLabel: "one@example.com",
        usedPercent: 100,
        resetAt: "2026-04-12T15:00:00Z",
        status: "critical",
        recommended: false,
      },
      {
        quotaId: "q2",
        accountId: "acct-2",
        accountLabel: "two@example.com",
        usedPercent: 20,
        resetAt: "2026-04-12T16:00:00Z",
        status: "normal",
        recommended: true,
      },
    ],
  };

  assert.equal(quotaPercent(pool.usedPercent, "consumed"), 60);
  assert.equal(quotaPercent(pool.usedPercent, "remaining"), 40);
  assert.equal(quotaPercentLabel(pool.usedPercent, "consumed"), "60%");
  assert.equal(quotaPercentLabel(pool.usedPercent, "remaining"), "40%");

  assert.deepEqual(
    visibleQuotaPools([pool], pool.vendor, [{ id: "q1" }, { id: "q2" }]),
    [pool],
  );
  assert.deepEqual(visibleQuotaPools([pool], pool.vendor, [{ id: "q2" }]), []);
});
