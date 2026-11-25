import { test } from "node:test";
import assert from "node:assert/strict";
import type { Quota } from "@agent-hud/protocol";
import {
  inRowOrder,
  quotaRowHidden,
  quotaRowKey,
  withRowShown,
} from "../src/quota-rows.ts";

const account = "account:aaa";
const other = "account:bbb";
const row = (window: string, accountId = account): Quota =>
  ({
    id: `claude:${accountId}:${window}`,
    vendor: "claude",
    name: window,
    usedPercent: 0,
    status: "normal",
    accountId,
    legacyWindowId: `${accountId}/${window}`,
  }) as Quota;

const session = row("claude-session");
const weekly = row("claude-weekly");
const fable = row("claude-weekly-fable");
const otherFable = row("claude-weekly-fable", other);

test("a layout writes a row by its Swift window id", () => {
  assert.equal(quotaRowKey(fable), `${account}/claude-weekly-fable`);
});

test("a row is hidden by its id, its window id, or its window alone", () => {
  assert.ok(quotaRowHidden(fable, [fable.id]));
  assert.ok(quotaRowHidden(fable, [`${account}/claude-weekly-fable`]));
  assert.ok(quotaRowHidden(fable, ["claude-weekly-fable"]));
  assert.ok(!quotaRowHidden(weekly, ["claude-weekly-fable"]));
});

test("rows follow a layout's order whichever key names them", () => {
  const ordered = inRowOrder(
    [session, weekly, fable],
    [fable.id, `${account}/claude-weekly`],
  );
  assert.deepEqual(
    ordered.map((r) => r.name),
    ["claude-weekly-fable", "claude-weekly", "claude-session"],
  );
});

test("showing a row hidden by its window alone keeps the other account's row hidden", () => {
  const hidden = withRowShown(["claude-weekly-fable", session.id], fable, [
    session,
    fable,
    otherFable,
  ]);
  assert.deepEqual(hidden, [session.id, `${other}/claude-weekly-fable`]);
  assert.ok(!quotaRowHidden(fable, hidden));
  assert.ok(quotaRowHidden(otherFable, hidden));
});

const agRow = (window: string, hash: string, accountId: string): Quota =>
  ({
    id: `antigravity:${hash}:${window}`,
    windowKey: `antigravity:${window}`,
    vendor: "antigravity",
    name: window,
    usedPercent: 0,
    status: "normal",
    accountId,
  }) as Quota;

test("an account-free windowKey keeps row order and hidden rows across accounts", () => {
  const aWeekly = agRow("gemini-weekly", "aaaa", account);
  const aFive = agRow("gemini-5h", "aaaa", account);
  const aThird = agRow("3p-5h", "aaaa", account);
  const bWeekly = agRow("gemini-weekly", "bbbb", other);
  const bFive = agRow("gemini-5h", "bbbb", other);
  const bThird = agRow("3p-5h", "bbbb", other);

  const savedOrder = [quotaRowKey(aFive), quotaRowKey(aWeekly)];
  const savedHidden = [quotaRowKey(aThird)];
  assert.deepEqual(savedOrder, [
    "antigravity:gemini-5h",
    "antigravity:gemini-weekly",
  ]);
  assert.deepEqual(savedHidden, ["antigravity:3p-5h"]);

  assert.deepEqual(
    inRowOrder([bWeekly, bFive, bThird], savedOrder).map((r) => r.name),
    ["gemini-5h", "gemini-weekly", "3p-5h"],
  );
  assert.ok(quotaRowHidden(bThird, savedHidden));
  assert.ok(!quotaRowHidden(bFive, savedHidden));

  // Layouts saved with earlier account-hashed ids keep matching that account.
  assert.deepEqual(
    inRowOrder([aWeekly, aFive], [aFive.id, aWeekly.id]).map((r) => r.name),
    ["gemini-5h", "gemini-weekly"],
  );
  assert.ok(quotaRowHidden(aThird, [aThird.id]));

  // Unhiding a row shared by windowKey clears it even when two accounts are present.
  const shown = withRowShown(["antigravity:3p-5h", session.id], aThird, [
    aThird,
    bThird,
    session,
  ]);
  assert.deepEqual(shown, [session.id]);
  assert.ok(!quotaRowHidden(aThird, shown));
  assert.ok(!quotaRowHidden(bThird, shown));
});
