import { test } from "node:test";
import assert from "node:assert/strict";
import {
  accountReadLabel,
  missingReadingNotice,
  nextExpiryLabel,
} from "../src/panel/account-labels.ts";

const now = Date.parse("2026-10-21T12:00:00Z");
const account = {
  id: "codex:a",
  provider: "codex",
  label: "me@example.com",
  plan: "pro",
  stale: false,
};

test("an account reads as current until a read fails, then says when it was last read", () => {
  assert.equal(
    accountReadLabel(
      { ...account, checkedAt: "2026-10-21T11:58:00Z" },
      now,
      "en",
    ),
    "Current account",
  );
  assert.equal(
    accountReadLabel(
      { ...account, checkedAt: "2026-10-21T11:58:00Z" },
      now,
      "zh",
    ),
    "当前账户",
  );
  assert.equal(
    accountReadLabel(
      { ...account, stale: true, checkedAt: "2026-10-21T09:00:00Z" },
      now,
      "en",
    ),
    "Last read 3h ago",
  );
  assert.equal(
    accountReadLabel(
      { ...account, stale: true, checkedAt: "2026-10-21T11:30:00Z" },
      now,
      "zh",
    ),
    "上次读取 30m 前",
  );
});

test("the first expiry is named only when every reset has one", () => {
  const credits = [
    { id: "a", expiresAt: "2026-11-02T08:00:00Z" },
    { id: "b", expiresAt: "2026-11-09T08:00:00Z" },
  ];
  assert.deepEqual(nextExpiryLabel({ availableCount: 2, credits }, "en"), {
    full: "First expires Nov 2",
    short: "Nov 2",
  });
  assert.equal(
    nextExpiryLabel({ availableCount: 3, credits }, "en").full,
    "Partial expiry details",
  );
  assert.equal(
    nextExpiryLabel({ availableCount: 1 }, "zh").full,
    "有效期未提供",
  );
});

test("a block without readings says why in its own provider's words", () => {
  const signedOut =
    "Claude Code is signed out. Run claude to sign in again to update the quota";
  const providers = [
    { id: "claude", notice: signedOut },
    { id: "codex", notice: null },
    { id: "cursor", notice: "  " },
  ];
  assert.equal(missingReadingNotice(providers, "claude"), signedOut);
  assert.equal(missingReadingNotice(providers, "codex"), null);
  assert.equal(missingReadingNotice(providers, "cursor"), null);
  assert.equal(missingReadingNotice(providers, "Claude"), null);
  assert.equal(missingReadingNotice(providers, "kimi"), null);
});
