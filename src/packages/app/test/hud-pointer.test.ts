import { test } from "node:test";
import assert from "node:assert/strict";
import {
  regionRectForElement,
  sameNativePointer,
} from "../src/lib/use-hud-pointer.ts";

test("sameNativePointer avoids state churn when native pointer snapshots match", () => {
  const base = {
    inside: true,
    point: { x: 420, y: 16 },
    buttons: 0,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
  };
  assert.equal(
    sameNativePointer(base, { ...base, point: { x: 420, y: 16 } }),
    true,
  );
  assert.equal(
    sameNativePointer(base, { ...base, point: { x: 421, y: 16 } }),
    false,
  );
  assert.equal(sameNativePointer(base, { ...base, inside: false }), false);
  assert.equal(sameNativePointer(base, { ...base, altKey: true }), false);
});

test("regionRectForElement prefers static data-target geometry over mid-tween DOM readbacks", () => {
  let domReads = 0;
  const animatedSurface = {
    dataset: {
      targetX: "180",
      targetY: "0",
      targetWidth: "540",
      targetHeight: "360",
    },
    getBoundingClientRect() {
      domReads += 1;
      return { x: 300, y: 100, width: 200, height: 80 };
    },
  };
  assert.deepEqual(
    regionRectForElement(animatedSurface, { left: 100, top: 0 }),
    { x: 180, y: 0, width: 540, height: 360 },
  );
  assert.equal(domReads, 0);

  const staticStrip = {
    dataset: {},
    getBoundingClientRect() {
      domReads += 1;
      return { x: 470, y: 0, width: 200, height: 24 };
    },
  };
  assert.deepEqual(regionRectForElement(staticStrip, { left: 100, top: 0 }), {
    x: 370,
    y: 0,
    width: 200,
    height: 24,
  });
  assert.equal(domReads, 1);
});
