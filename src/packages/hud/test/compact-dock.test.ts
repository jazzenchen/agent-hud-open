import { test } from "node:test";
import assert from "node:assert/strict";
import { compactLabel, compactVendors } from "../src/compact-dock.ts";

const idle = {
  used: null,
  heldUsed: null,
  balance: null,
  usage: null,
  working: 0,
  waiting: 0,
  level: "stale",
};

test("compact dock keeps known zero usage and monetary balances", () => {
  const zero = { ...idle, used: 0 };
  const balance = { ...idle, balance: "¥8.97" };
  assert.deepEqual(compactVendors([idle, zero, balance]), [zero, balance]);
});

test("unknown readings never hide working agents or requests", () => {
  const working = { ...idle, working: 1 };
  const waiting = { ...idle, waiting: 1 };
  const critical = { ...idle, level: "critical" };
  assert.deepEqual(compactVendors([idle, working, waiting, critical]), [
    working,
    waiting,
    critical,
  ]);
});

test("all-idle dock retains one entry point, without mutating the full list", () => {
  const vendors = [idle, { ...idle }];
  assert.deepEqual(compactVendors(vendors), [idle]);
  assert.equal(vendors.length, 2);
  assert.deepEqual(compactVendors([]), []);
});

test("compact status preserves known labels and distinguishes activity", () => {
  assert.equal(compactLabel(idle, "18%"), "18%");
  assert.equal(compactLabel(idle, "¥8.97"), "¥8.97");
  assert.equal(compactLabel({ ...idle, waiting: 1 }, "--"), "!");
  assert.equal(compactLabel({ ...idle, working: 1 }, "--"), "…");
});
