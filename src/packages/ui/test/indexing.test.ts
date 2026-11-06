import { test } from "node:test";
import assert from "node:assert/strict";
import { indexingText } from "../src/text/indexing.ts";

const now = new Date(2026, 10, 6, 12);
const at = (year: number, month: number, day: number, hour = 9) =>
  new Date(year, month, day, hour).toISOString();

test("nothing shows outside a run", () => {
  assert.equal(indexingText(undefined, "en"), null);
  assert.equal(
    indexingText({ kind: null, bytesDone: 1, bytesTotal: 2 }, "en"),
    null,
  );
});

test("a run says how far back it has read, by day", () => {
  const facts = {
    kind: "reindex",
    oldestLogAt: at(2026, 4, 7),
    readBackTo: at(2026, 8, 12, 23),
  };
  assert.deepEqual(indexingText(facts, "en", now), {
    fraction: 55 / 184,
    title: "Reading your history again · back to Sep 12",
    days: "55 of 184 days",
    detail:
      "Your logs go back to May 7 · tokens and costs are incomplete until it finishes",
  });
  const zh = indexingText({ ...facts, kind: "first" }, "zh", now);
  assert.equal(zh?.title, "正在读取历史记录 · 已读到 9月12日");
  assert.equal(zh?.days, "55/184 天");
  assert.equal(zh?.detail, "日志可追溯到 5月7日 · 完成前 Token 和费用不完整");
});

test("a first run starts at today and names another year", () => {
  const text = indexingText(
    {
      kind: "first",
      oldestLogAt: at(2025, 9, 7),
      readBackTo: at(2026, 10, 6, 8),
    },
    "en",
    now,
  );
  assert.equal(text?.fraction, 0);
  assert.equal(text?.title, "Reading your history · back to Nov 6");
  assert.match(text?.detail ?? "", /^Your logs go back to Oct 7, 2025 · /);
});

test("a run shows less than 100% until the core ends it", () => {
  const text = indexingText(
    { kind: "reindex", oldestLogAt: at(2026, 4, 7), readBackTo: null },
    "zh",
    now,
  );
  assert.equal(text?.fraction, 0.99);
  assert.equal(text?.title, "正在重新读取历史记录 · 已读到 5月7日");
});

test("without days it falls back on bytes", () => {
  assert.deepEqual(
    indexingText({ kind: "reindex", bytesDone: 450, bytesTotal: 1_000 }, "en"),
    {
      fraction: 0.45,
      title: "Reading your history again",
      days: "",
      detail: "Tokens and costs are incomplete until it finishes",
    },
  );
  assert.equal(
    indexingText({ kind: "first", bytesDone: 0, bytesTotal: 0 }, "zh")?.title,
    "正在读取历史记录",
  );
});
