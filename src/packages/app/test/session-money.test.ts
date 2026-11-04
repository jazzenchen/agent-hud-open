import { test } from "node:test";
import assert from "node:assert/strict";
import { sessionMoney, unpricedHelp } from "../src/lib/sessions.ts";

test("a whole cost is the session's money, with nothing left out", () => {
  assert.deepEqual(
    sessionMoney({ cost: 1.5, pricedCost: 1.5, unpricedTokens: 0 }),
    { value: 1.5, unpriced: 0 },
  );
});

test("a partly priced session keeps its priced calls' cost and counts the rest", () => {
  assert.deepEqual(
    sessionMoney({ cost: null, pricedCost: 0.4, unpricedTokens: 12_000 }),
    { value: 0.4, unpriced: 12_000 },
  );
  assert.deepEqual(
    sessionMoney({ cost: null, pricedCost: null, unpricedTokens: 900 }),
    { value: null, unpriced: 900 },
    "only unpriced calls leave no amount",
  );
  assert.deepEqual(
    sessionMoney({ cost: null }),
    { value: null, unpriced: 0 },
    "a row from before the fields existed",
  );
});

test("the tooltip names what a cost leaves out", () => {
  assert.equal(unpricedHelp(0), undefined);
  assert.equal(
    unpricedHelp(12_000, false, ["Claude Code · qwen3-coder:30b"]),
    "12,000 more tokens came from models with no API list price and are not included: Claude Code · qwen3-coder:30b",
  );
  assert.equal(
    unpricedHelp(12_000, true),
    "另有 12,000 个 Token 来自没有 API 价格的模型，未计入",
  );
});
