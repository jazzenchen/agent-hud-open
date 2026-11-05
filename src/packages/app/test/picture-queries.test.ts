import { test } from "node:test";
import assert from "node:assert/strict";
import { maskedAnswer, PictureQueries } from "../src/lib/picture-queries.ts";

test("a picture waits for answers that come in time and names none", async () => {
  const queries = new PictureQueries();
  queries.track("statistics", Promise.resolve({ buckets: [] }));
  queries.track("sessions", Promise.reject(new Error("no index")));
  assert.deepEqual(await queries.settle(1_000), []);
});

test("a picture stops waiting at its deadline and names the sections still loading", async () => {
  const queries = new PictureQueries();
  queries.track("statistics", new Promise(() => {}));
  queries.track("sessions", Promise.resolve({ items: [] }));
  const started = Date.now();
  assert.deepEqual(await queries.settle(20), ["tokens"]);
  assert.ok(Date.now() - started < 1_000);
});

test("a deadline already past takes the picture at once", async () => {
  const queries = new PictureQueries();
  queries.track("sessions", new Promise(() => {}));
  assert.deepEqual(await queries.settle(-5_000), ["sessions"]);
});

test("the quota history and the plan value are named while loading and drawn without account names", async () => {
  const queries = new PictureQueries();
  queries.track("quota_history", new Promise(() => {}));
  queries.track("plan_value", new Promise(() => {}));
  assert.deepEqual(await queries.settle(10), ["quotaHistory", "planValue"]);
  const history = maskedAnswer("quota_history", {
    windows: [
      { id: "w1", accountId: "claude:a", account: "a@example.com" },
      { id: "w2", accountId: "claude:b", account: "b@example.com" },
      { id: "w3", accountId: "claude:a", account: "a@example.com" },
    ],
  }) as { windows: { account: string }[] };
  assert.deepEqual(
    history.windows.map((window) => window.account),
    ["Account 1", "Account 2", "Account 1"],
  );
  const value = maskedAnswer("plan_value", {
    accounts: [{ id: "claude:a", name: "a@example.com" }],
  }) as { accounts: { name: string }[] };
  assert.equal(value.accounts[0].name, "Account 1");
});
