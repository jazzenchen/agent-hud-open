import { test } from "node:test";
import assert from "node:assert/strict";
import { withMessagePage } from "../src/lib/sessions.ts";

test("a page of messages keeps the selected turn's calls", () => {
  const detail = {
    session: { id: "session-1" },
    turns: [{ id: "turn-1", calls: [{ id: "call-1" }] }],
    turnCalls: { turn: "turn-1", calls: [{ id: "call-1" }] },
    messages: [{ id: "m1" }],
    nextMessageCursor: 100,
    messagesComplete: false,
  };
  // Asked without the turn, a page's detail has no calls.
  const page = {
    session: { id: "session-1" },
    turns: [{ id: "turn-1" }],
    messages: [{ id: "m2" }],
    nextMessageCursor: null,
    messagesComplete: true,
  };
  const merged = withMessagePage(detail, page);
  assert.deepEqual(merged.messages, [{ id: "m1" }, { id: "m2" }]);
  assert.deepEqual(merged.turns, detail.turns);
  assert.deepEqual(merged.turnCalls, detail.turnCalls);
  assert.equal(merged.nextMessageCursor, null);
  assert.equal(merged.messagesComplete, true);
});
