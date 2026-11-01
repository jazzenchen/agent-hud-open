import { test } from "node:test";
import assert from "node:assert/strict";
import { heldUsed } from "../src/held-reading.ts";

const now = Date.parse("2026-11-01T12:00:00Z");
const window = (
  usedPercent: number | null,
  status: string,
  resetAt: string,
) => ({
  usedPercent,
  status,
  resetAt,
});

test("a failed read holds the tightest window that has not reset", () => {
  assert.equal(
    heldUsed(
      [
        window(12, "stale", "2026-11-01T13:00:00Z"),
        window(97, "stale", "2026-11-03T12:00:00Z"),
        window(100, "stale", "2026-11-01T11:00:00Z"),
      ],
      now,
    ),
    97,
  );
});

test("a live window stands for the mark, so nothing is held", () => {
  assert.equal(
    heldUsed(
      [
        window(12, "normal", "2026-11-01T13:00:00Z"),
        window(97, "stale", "2026-11-03T12:00:00Z"),
      ],
      now,
    ),
    null,
  );
});

test("windows that reset since, or never had a number, hold nothing", () => {
  assert.equal(
    heldUsed(
      [
        window(97, "stale", "2026-11-01T11:00:00Z"),
        window(null, "stale", "2026-11-03T12:00:00Z"),
      ],
      now,
    ),
    null,
  );
  assert.equal(heldUsed([], now), null);
});
