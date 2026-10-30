import { test } from "node:test";
import assert from "node:assert/strict";
import { visibleBalances } from "../src/balance-rows.ts";

const rows = [
  {
    id: "deepseek:1:USD",
    accountId: "deepseek:1",
    vendor: "deepseek",
    label: "API balance",
    currency: "USD",
    amount: 0,
    notice: null,
  },
  {
    id: "deepseek:1:CNY",
    accountId: "deepseek:1",
    vendor: "deepseek",
    label: "API balance",
    currency: "CNY",
    amount: 9.92,
    notice: null,
  },
];

test("balance row settings hide and order currencies independently within an account", () => {
  assert.deepEqual(
    visibleBalances(rows).map((r) => r.currency),
    ["USD", "CNY"],
  );
  assert.deepEqual(
    visibleBalances(rows, { rows: [rows[1].id, rows[0].id] }).map(
      (r) => r.currency,
    ),
    ["CNY", "USD"],
  );
  assert.deepEqual(
    visibleBalances(rows, { hiddenRows: [rows[0].id] }).map((r) => r.currency),
    ["CNY"],
  );
  assert.deepEqual(
    visibleBalances(rows, { hiddenRows: rows.map((r) => r.id) }),
    [],
  );
  assert.deepEqual(
    visibleBalances(rows, { hiddenAccounts: ["deepseek:1"] }),
    [],
  );
  assert.deepEqual(
    visibleBalances(rows, {
      rows: ["temporarily-missing"],
      hiddenRows: ["temporarily-missing"],
    }).map((r) => r.currency),
    ["USD", "CNY"],
  );
});
