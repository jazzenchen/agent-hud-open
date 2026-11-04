import { test } from "node:test";
import assert from "node:assert/strict";
import {
  agentArtwork,
  agentArtworkKey,
  agentVendorName,
} from "../src/agent-artwork.ts";

test("an agent without its own artwork keeps its vendor's name", () => {
  assert.equal(agentArtworkKey("trae-cn"), "trae");
  assert.equal(agentVendorName("trae"), "TRAE");
  assert.equal(agentVendorName("trae-cn"), "TRAE");
  // It is drawn with the fallback mark.
  assert.equal(agentArtwork("trae"), null);
  assert.equal(agentVendorName("qoder-cn"), "Qoder");
  assert.equal(agentVendorName("someone-new"), "someone-new");
});
