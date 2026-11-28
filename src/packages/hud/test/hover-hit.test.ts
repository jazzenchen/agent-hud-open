import { test } from "node:test";
import assert from "node:assert/strict";
import { expandedOrigin, isHoverHit } from "../src/scene/hover-hit.ts";

const notch = { x: 370, y: 0, width: 200, height: 24 };
const frame = { x: 184, y: 0, width: 572, height: 340 };

test("collapsed notch ignores the 14pt grab zone below and 8pt side flares (c06 #15)", () => {
  // Inside the visible notch strip (including its boundary): opens.
  assert.equal(
    isHoverHit({ x: 470, y: 12 }, { strip: notch, cardVisible: false, frame }),
    true,
  );
  assert.equal(
    isHoverHit({ x: 370, y: 24 }, { strip: notch, cardVisible: false, frame }),
    true,
  );
  // 1px to 14px below the notch (inside .hud-grab-surface, outside strip): ignored.
  assert.equal(
    isHoverHit({ x: 470, y: 25 }, { strip: notch, cardVisible: false, frame }),
    false,
  );
  assert.equal(
    isHoverHit({ x: 470, y: 36 }, { strip: notch, cardVisible: false, frame }),
    false,
  );
  // 8pt side flares outside strip: ignored when collapsed.
  assert.equal(
    isHoverHit({ x: 365, y: 10 }, { strip: notch, cardVisible: false, frame }),
    false,
  );
  assert.equal(
    isHoverHit({ x: 575, y: 10 }, { strip: notch, cardVisible: false, frame }),
    false,
  );
});

test("collapsed dock ignores outer grab margins, uses foldedZone when folded, and honours visible dock ends", () => {
  const dock = { x: 400, y: 0, width: 140, height: 29 };
  // 6px past the dock's right end (inside ±8 grab surface): ignored.
  assert.equal(
    isHoverHit({ x: 546, y: 14 }, { strip: dock, cardVisible: false, frame }),
    false,
  );
  // Visible dock end pill beside the dock: counts as hovering (with 3pt pad).
  const tailEnd = { x: 546, y: 4, width: 22, height: 22 };
  assert.equal(
    isHoverHit(
      { x: 544, y: 14 },
      { strip: dock, cardVisible: false, frame, endRects: [tailEnd] },
    ),
    true,
  );
  // Folded dock: only the edge rest zone counts, not the unfolded strip depth.
  const foldedZone = { x: 420, y: 0, width: 100, height: 4 };
  assert.equal(
    isHoverHit(
      { x: 470, y: 20 },
      { strip: dock, cardVisible: false, frame, foldedZone },
    ),
    false,
  );
  assert.equal(
    isHoverHit(
      { x: 470, y: 3 },
      { strip: dock, cardVisible: false, frame, foldedZone },
    ),
    true,
  );
});

test("expanded card keeps hover across frame, strip, and shrinking heldFrame", () => {
  const heldFrame = { x: 184, y: 0, width: 572, height: 460 };
  assert.equal(
    isHoverHit({ x: 300, y: 280 }, { strip: notch, cardVisible: true, frame }),
    true,
  );
  assert.equal(
    isHoverHit(
      { x: 300, y: 420 },
      { strip: notch, cardVisible: true, frame, heldFrame },
    ),
    true,
  );
});

test("expandedOrigin anchors GPU scale to the strip centre even when clamped near a corner", () => {
  const cornerStrip = { x: 20, y: 0, width: 120, height: 28 };
  const clampedFrame = { x: 0, y: 0, width: 540, height: 320 };
  assert.equal(expandedOrigin("top", clampedFrame, cornerStrip), "80px 0px");
  assert.equal(
    expandedOrigin(
      "bottom",
      { x: 0, y: 760, width: 540, height: 320 },
      { x: 20, y: 1052, width: 120, height: 28 },
    ),
    "80px 100%",
  );
  assert.equal(
    expandedOrigin(
      "right",
      { x: 400, y: 600, width: 540, height: 320 },
      { x: 900, y: 800, width: 40, height: 100 },
    ),
    "100% 250px",
  );
});
