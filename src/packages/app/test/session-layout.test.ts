import { test } from "node:test";
import assert from "node:assert/strict";
import {
  sessionColumns,
  sessionLayout,
} from "../src/sessions/session-layout.ts";

test("the page steps from one column to two to three as it widens", () => {
  assert.equal(sessionLayout(720), "one");
  assert.equal(sessionLayout(999), "one");
  assert.equal(sessionLayout(1000), "two");
  assert.equal(sessionLayout(1280), "two");
  assert.equal(sessionLayout(1440), "three");
  assert.equal(sessionLayout(1800), "three");
});

test("three columns never squeeze the session below 480 points of content", () => {
  for (let width = 1000; width <= 2400; width += 1) {
    if (sessionLayout(width) !== "three") continue;
    const { tracks } = sessionColumns(width, true, true);
    const [list, pick] = tracks.split(" minmax(0, 1fr) ");
    const session = width - 22 - parseInt(list) - parseInt(pick);
    assert.ok(session - 44 >= 480, `${width}: ${session}`);
  }
});

test("one column shows the deepest thing open", () => {
  assert.deepEqual(sessionColumns(800, false, false).shown, ["list"]);
  assert.deepEqual(sessionColumns(800, true, false).shown, ["session"]);
  assert.deepEqual(sessionColumns(800, true, true).shown, ["pick"]);
});

test("two columns let a pick take the list's place", () => {
  assert.deepEqual(sessionColumns(1280, true, false).shown, [
    "list",
    "session",
  ]);
  const picked = sessionColumns(1280, true, true);
  assert.deepEqual(picked.shown, ["session", "pick"]);
  assert.equal(picked.list, 0);
});

test("three columns keep the list beside the session and the pick", () => {
  const picked = sessionColumns(1440, true, true);
  assert.deepEqual(picked.shown, ["list", "session", "pick"]);
  assert.ok(picked.list >= 424);
  // No session picked: a pick alone has nowhere to sit.
  assert.deepEqual(sessionColumns(1440, false, true).shown, [
    "list",
    "session",
  ]);
});
