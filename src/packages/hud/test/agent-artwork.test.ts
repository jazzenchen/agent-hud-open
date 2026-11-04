import { test } from "node:test";
import assert from "node:assert/strict";
import {
  agentArtwork,
  agentArtworkKey,
  agentVendorName,
} from "../src/agent-artwork.ts";

test("an agent without its own artwork keeps its vendor's name", () => {
  assert.equal(agentVendorName("droid"), "Factory Droid");
  // It is drawn with the fallback mark.
  assert.equal(agentArtwork("droid"), null);
  assert.equal(agentVendorName("qoder-cn"), "Qoder");
  assert.equal(agentVendorName("someone-new"), "someone-new");
});

test("TRAE and TRAE CN share TRAE's mark, tinted to the foreground", () => {
  assert.equal(agentArtworkKey("trae-cn"), "trae");
  assert.equal(agentVendorName("trae-cn"), "TRAE");
  for (const source of ["trae", "trae-cn"]) {
    const artwork = agentArtwork(source);
    assert.equal(artwork?.file, "trae");
    assert.equal(artwork?.template, true);
    assert.equal(artwork?.tint, "#ffffff");
  }
  assert.equal(agentArtwork("trae", true)?.tint, "#1d1d1f");
});
