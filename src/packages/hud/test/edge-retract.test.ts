import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EDGE_RETRACT_DELAY_MS,
  canRetractAtEdge,
  compactStripMetrics,
  retractedStripFrame,
  type EdgeRetractGate,
} from "../src/scene/edge-retract.ts";

const idleGate: EdgeRetractGate = {
  mode: "simple",
  autoHideAtEdge: true,
  preview: false,
  open: false,
  pinned: false,
  pointerInside: false,
  cardVisible: false,
  dragVisual: false,
  hasAlert: false,
  hasOpenEnd: false,
};

test("canRetractAtEdge only enables retract for idle non-preview Simple docks", () => {
  assert.equal(EDGE_RETRACT_DELAY_MS, 350);
  assert.equal(canRetractAtEdge(idleGate), true);

  for (const [label, override] of [
    ["notch mode", { mode: "notch" as const }],
    ["setting disabled", { autoHideAtEdge: false }],
    ["drag/wallpaper preview", { preview: true }],
    ["panel open", { open: true }],
    ["panel pinned", { pinned: true }],
    ["pointer inside", { pointerInside: true }],
    ["card visible", { cardVisible: true }],
    ["dragging", { dragVisual: true }],
    ["active alert", { hasAlert: true }],
    ["open dock end", { hasOpenEnd: true }],
  ] as const) {
    assert.equal(
      canRetractAtEdge({ ...idleGate, ...override }),
      false,
      `should not retract when ${label}`,
    );
  }
});

test("compactStripMetrics scales proportionally with Simple size and gap settings", () => {
  const small = compactStripMetrics({ simpleSize: 20 }, { gap: 2, radius: 11 });
  assert.deepEqual(small, { mark: 14, gap: 2, pad: 5, radius: 11 });

  const medium = compactStripMetrics(
    { simpleSize: 26 },
    { gap: 7, radius: 14 },
  );
  assert.deepEqual(medium, { mark: 18, gap: 5, pad: 5, radius: 14 });

  const large = compactStripMetrics(
    { simpleSize: 44 },
    { gap: 11, radius: 32 },
  );
  assert.deepEqual(large, { mark: 30, gap: 8, pad: 9, radius: 24 });

  const max = compactStripMetrics({ simpleSize: 60 }, { gap: 36, radius: 44 });
  assert.deepEqual(max, { mark: 41, gap: 27, pad: 12, radius: 33 });
});

test("retractedStripFrame encompasses all icons, gaps, and padding on every edge", () => {
  const medium = compactStripMetrics(
    { simpleSize: 26 },
    { gap: 7, radius: 14 },
  );
  // 4 marks: 5*2 + 4*18 + 3*5 = 97px span, 5*2 + 18 = 28px thickness.
  assert.deepEqual(
    retractedStripFrame(
      { x: 1840, y: 300, width: 80, height: 140 },
      "right",
      4,
      medium,
    ),
    { x: 1892, y: 322, width: 28, height: 97 },
  );
  assert.deepEqual(
    retractedStripFrame(
      { x: 0, y: 300, width: 80, height: 140 },
      "left",
      4,
      medium,
    ),
    { x: 0, y: 322, width: 28, height: 97 },
  );

  // Large Simple size (60): mark=41, gap=27, pad=12 -> 4 marks span 12*2 + 4*41 + 3*27 = 269, thickness = 65.
  const max = compactStripMetrics({ simpleSize: 60 }, { gap: 36, radius: 44 });
  assert.deepEqual(
    retractedStripFrame(
      { x: 1800, y: 200, width: 120, height: 320 },
      "right",
      4,
      max,
    ),
    { x: 1855, y: 226, width: 65, height: 269 },
  );
  assert.deepEqual(
    retractedStripFrame(
      { x: 400, y: 0, width: 320, height: 90 },
      "top",
      4,
      max,
    ),
    { x: 426, y: 0, width: 269, height: 65 },
  );
  assert.deepEqual(
    retractedStripFrame(
      { x: 400, y: 990, width: 320, height: 90 },
      "bottom",
      4,
      max,
    ),
    { x: 426, y: 1015, width: 269, height: 65 },
  );

  // 0 vendors and short strips clamp cleanly.
  assert.deepEqual(
    retractedStripFrame(
      { x: 1840, y: 300, width: 80, height: 140 },
      "right",
      0,
      medium,
    ),
    { x: 1892, y: 352, width: 28, height: 36 },
  );
  assert.deepEqual(
    retractedStripFrame(
      { x: 1890, y: 500, width: 20, height: 24 },
      "right",
      4,
      medium,
    ),
    { x: 1890, y: 500, width: 20, height: 24 },
  );
});
