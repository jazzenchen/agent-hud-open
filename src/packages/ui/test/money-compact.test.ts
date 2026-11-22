import { test } from "node:test";
import assert from "node:assert/strict";
import { moneyCompact } from "../src/format.ts";

test("a balance in the dock keeps decimals only below ten, then k, M and B", () => {
  const cases: [number | string, string][] = [
    ["5.37", "¥5.37"],
    [0.4, "¥0.40"],
    ["21.46", "¥21"],
    ["104.58", "¥105"],
    [999.4, "¥999"],
    [999.6, "¥1.0k"],
    [1234.5, "¥1.2k"],
    [48_000, "¥48k"],
    [2_400_000, "¥2.4M"],
    [-1500, "-¥1.5k"],
  ];
  for (const [amount, text] of cases)
    assert.equal(moneyCompact(amount, "CNY", "en"), text, String(amount));
  assert.equal(moneyCompact(12_345, "USD", "en"), "$12k");
  assert.equal(moneyCompact(12_345, "credits", "en"), "12k");
  // The symbol stays where the language writes it.
  assert.equal(moneyCompact(12_345, "EUR", "de"), "12k\u00a0€");
});
