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
  assert.equal(agentVendorName("qoder-cn"), "Qoder CN");
  assert.equal(agentVendorName("QoderWork"), "QoderWork");
  assert.equal(agentVendorName("codebuddy-cn"), "CodeBuddy CN");
  assert.equal(agentVendorName("minimax-code"), "MiniMax Code");
  assert.equal(agentVendorName("someone-new"), "someone-new");
});

test("separate domestic providers keep distinct keys while sharing brand artwork", () => {
  assert.equal(agentArtworkKey("trae"), "trae");
  assert.equal(agentArtworkKey("trae-cn"), "trae-cn");
  assert.equal(agentVendorName("trae-cn"), "TRAE CN");
  for (const source of ["trae", "trae-cn"]) {
    const artwork = agentArtwork(source);
    assert.equal(artwork?.file, "trae");
    assert.equal(artwork?.template, true);
    assert.equal(artwork?.tint, "#ffffff");
  }
  assert.equal(agentArtwork("trae", true)?.tint, "#1d1d1f");

  assert.equal(agentArtworkKey("qoder-cn"), "qoder-cn");
  assert.equal(agentArtworkKey("qoder-work"), "qoder-work");
  assert.equal(agentArtworkKey("QoderWork"), "qoder-work");
  assert.equal(agentArtwork("qoder-cn", false)?.file, "qoder-dark");
  assert.equal(agentArtwork("qoder-work", true)?.file, "qoder");
  assert.equal(agentArtwork("codebuddy-cn")?.file, "codebuddy");
  assert.equal(agentArtwork("minimax-code")?.file, "minimax");
});
