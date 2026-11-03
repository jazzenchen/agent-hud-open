import { test } from "node:test";
import assert from "node:assert/strict";
import type { Quota } from "@agent-hud/protocol";
import {
  arcPath,
  simpleLogoSize,
  simpleRingSpec,
  usageArcs,
} from "../src/usage-meter/arcs.ts";
import { markWindows } from "../src/usage-meter/mark-windows.ts";

const spec = { size: 60, width: 5, hole: 30, gap: 3 };

test("segments split one ring: halves, thirds with one on top, quarters", () => {
  const centres = (n: number) =>
    usageArcs("segments", n, spec).map((a) => (a.start + a.end) / 2);
  assert.deepEqual(centres(2), [0, 180]);
  assert.deepEqual(centres(3), [0, 120, 240]);
  assert.deepEqual(centres(4), [-45, 45, 135, 225]);
  const [one] = usageArcs("segments", 1, spec);
  assert.deepEqual([one.start, one.end], [0, 360]);
  // Round ends leave `gap` clear between two segments.
  const [top, bottom] = usageArcs("segments", 2, spec);
  const clear = ((bottom.start - top.end) * Math.PI * top.radius) / 180;
  assert.ok(Math.abs(clear - (spec.gap + spec.width)) < 1e-9);
});

test("rings run from the outside in and thin out to fit the band", () => {
  const two = usageArcs("rings", 2, spec);
  assert.deepEqual(
    two.map((a) => [a.radius, a.width]),
    [
      [27.5, 5],
      [20, 5],
    ],
  );
  const four = usageArcs("rings", 4, spec);
  const band =
    four[0].radius + four[0].width / 2 - (four[3].radius - four[3].width / 2);
  assert.ok(four[0].width < 5);
  assert.ok(Math.abs(band - (spec.size - spec.hole) / 2) < 1e-9);
  assert.equal(usageArcs("rings", 9, spec).length, 4);
});

test("a Simple mark keeps 55% for its logo until a third ring comes in", () => {
  const size = 22,
    ring = simpleRingSpec(size, 1.5);
  const logo = (n: number) =>
    simpleLogoSize(size, usageArcs("rings", n, ring)) / size;
  assert.equal(logo(1), 0.55);
  assert.equal(logo(2), 0.55);
  assert.ok(logo(3) > 0.44 && logo(3) < 0.48);
  assert.ok(logo(4) > 0.44 && logo(4) < 0.48);
});

test("an arc path is a whole closed ring or one sweep", () => {
  assert.match(
    arcPath(10, { radius: 8 }, 0, 360),
    /^M10 2A8 8 0 1 1 10 18A8 8 0 1 1 10 2Z$/,
  );
  assert.equal(arcPath(10, { radius: 8 }, 0, 90), "M10 2A8 8 0 0 1 18 10");
  assert.equal(arcPath(10, { radius: 8 }, 30, 30), "");
});

const row = (
  id: string,
  account: string,
  used: number | null,
  status = "normal",
): Quota => ({
  id,
  vendor: "claude",
  name: id,
  accountId: account,
  account,
  usedPercent: used,
  resetAt: "2099-01-01T00:00:00Z",
  status: status as Quota["status"],
});

test("a mark draws the windows of the account nearest its limit, in the panel's order", () => {
  const quotas = [
    row("a/5h", "a", 12),
    row("a/week", "a", 38),
    row("b/5h", "b", 93, "critical"),
    row("b/week", "b", 71, "warning"),
    row("b/sonnet", "b", 52),
  ];
  assert.deepEqual(
    markWindows({ quotas }).map((q) => q.id),
    ["b/5h", "b/week", "b/sonnet"],
  );
  const layout = {
    blocks: { claude: { hiddenRows: ["b/sonnet"], rows: ["b/week"] } },
  };
  assert.deepEqual(
    markWindows({ quotas }, layout).map((q) => q.id),
    ["b/week", "b/5h"],
  );
  // While every window waits on a failed read, its last reading picks the account.
  const held = quotas.map((q) => ({ ...q, status: "stale" as const }));
  assert.deepEqual(
    markWindows({ quotas: held }).map((q) => q.id),
    ["b/5h", "b/week", "b/sonnet"],
  );
  assert.deepEqual(markWindows({ quotas: [] }), []);
});
