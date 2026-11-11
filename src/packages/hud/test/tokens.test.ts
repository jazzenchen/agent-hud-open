import { test } from "node:test";
import assert from "node:assert/strict";
import {
  alertFill,
  alertInk,
  alertText,
  hudPalette,
  idleText,
  lookGround,
  markLevelText,
  onAlertFill,
  permissionColors,
} from "../src/tokens.ts";

const dark = { light: false },
  light = { light: true },
  thinDark = { light: false, seeThrough: true },
  thinLight = { light: true, seeThrough: true };

test("alert accents and ink follow Theme", () => {
  assert.equal(alertFill("calm", dark), "#6cd8ac");
  assert.equal(alertFill("turn", light), "#2f62d0");
  assert.equal(alertText("warning", thinLight), "#7c3b12");
  assert.equal(onAlertFill(dark), "#000000");
  assert.equal(onAlertFill(light), "#ffffff");
  assert.equal(alertInk(0.5, light), "rgba(29,29,31,0.7)");
  assert.equal(alertInk(0.5, dark), "rgba(255,255,255,0.5)");
});

test("permission controls follow PermissionColor", () => {
  assert.deepEqual(permissionColors(light), {
    allowFill: "#3ddc84",
    onAllow: "#000000",
    denyFill: "rgba(255,59,48,0.14)",
    denyEdge: "rgba(255,59,48,0.3)",
    denyText: "#d70015",
    alwaysEdge: "#1f9d4c",
    alwaysText: "#1e7e34",
  });
});

test("a look lets the desktop through under a solid with a transparency", () => {
  assert.deepEqual(lookGround({ solidTransparency: 0 }, false), {
    light: false,
    seeThrough: false,
  });
  assert.deepEqual(lookGround({ solidTransparency: 0.3 }, true), {
    light: true,
    seeThrough: true,
  });
});

test("on a see-through ground the greys are tints of the ink and levels take their see-through shades", () => {
  const darkThinned = hudPalette(thinDark);
  assert.equal(darkThinned.secondary, "rgba(255,255,255,0.6)");
  assert.equal(darkThinned.tertiary, "rgba(255,255,255,0.43)");
  assert.equal(darkThinned.text, "#f5f5f7");
  assert.equal(darkThinned.ok, "#3ddc84");
  assert.equal(darkThinned.okText, "#9df0c0");
  assert.equal(darkThinned.criticalText, "#ffa39b");
  const lightThinned = hudPalette(thinLight);
  assert.equal(lightThinned.secondary, "rgba(0,0,0,0.55)");
  assert.equal(lightThinned.tertiary, "rgba(0,0,0,0.42)");
  assert.equal(lightThinned.warningText, "#6e4900");
});

test("a mark's figure takes its level's text shade, or the idle grey's", () => {
  assert.equal(markLevelText("normal", dark), "#3ddc84");
  assert.equal(markLevelText("warning", light), "#946300");
  assert.equal(markLevelText("stale", dark), "#9a9aa0");
  assert.equal(markLevelText("stale", light), "#6e6e73");
  assert.equal(markLevelText("normal", thinDark), "#9df0c0");
  assert.equal(idleText(thinDark), "rgba(255,255,255,0.6)");
});
