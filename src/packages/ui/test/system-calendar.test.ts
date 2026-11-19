import { test } from "node:test";
import assert from "node:assert/strict";
import { systemCalendar } from "../src/format.ts";

test("a locale Intl refuses falls back to a Monday week instead of throwing", () => {
  // WebKitGTK reports the language "c" on Linux with LANG=C.UTF-8 or POSIX.
  for (const tag of ["c", "C", "posix", ""]) {
    assert.equal(systemCalendar(tag).firstWeekday, 2, tag);
  }
});

test("a valid locale keeps its own first day of the week", () => {
  assert.equal(systemCalendar("en-US").firstWeekday, 1);
  assert.equal(systemCalendar("de-DE").firstWeekday, 2);
});
