import { test } from "node:test";
import assert from "node:assert/strict";
import {
  SURFACE_BORDER_PX,
  bodyMaxHeight,
  expandedPanelHeight,
  isSurfaceBordered,
  peekBubbleHeight,
} from "../src/scene/surface-size.ts";

test("isSurfaceBordered matches detached cards and non-simple-peek Simple surfaces", () => {
  assert.equal(isSurfaceBordered("simple", false, false), true);
  assert.equal(isSurfaceBordered("simple", true, false), true);
  assert.equal(isSurfaceBordered("simple", true, true), false);
  assert.equal(isSurfaceBordered("notch", false, false), false);
  assert.equal(isSurfaceBordered("notch", true, false), true);
});

test("peekBubbleHeight includes empty Quota fallback row and border allowance", () => {
  // Title (18) + empty "Quota --" fallback row (18) + 20px vertical padding = 56, plus 2px border.
  assert.equal(
    peekBubbleHeight({ quotas: [], balance: null }, 1, true),
    56 + SURFACE_BORDER_PX,
  );
  assert.equal(peekBubbleHeight({ quotas: [], balance: null }, 1, false), 56);
  // Two quotas (e.g. OpenAI 5h + Weekly) without balance: 1 title + 2 quotas = 3 rows -> 20 + 54 = 74 (+ 2 border).
  assert.equal(
    peekBubbleHeight({ quotas: [{}, {}], balance: null }, 1, true),
    74 + SURFACE_BORDER_PX,
  );
  // Balance only (e.g. DeepSeek): 1 title + 1 balance = 2 rows -> 20 + 36 = 56 (+ 2 border).
  assert.equal(
    peekBubbleHeight({ quotas: [], balance: "$0.00" }, 1, true),
    56 + SURFACE_BORDER_PX,
  );
});

test("expandedPanelHeight reserves 2px for bordered panels without bypassing floor/screen clamp", () => {
  assert.equal(expandedPanelHeight(300, 900, true), 302);
  assert.equal(expandedPanelHeight(300, 900, false), 300);
  assert.equal(expandedPanelHeight(40, 900, true), 80);
  assert.equal(expandedPanelHeight(820, 900, true), 820);
  assert.equal(bodyMaxHeight(302, { top: 14, bottom: 14 }, true), 300 - 28);
  assert.equal(bodyMaxHeight(300, { top: 14, bottom: 14 }, false), 300 - 28);
});
