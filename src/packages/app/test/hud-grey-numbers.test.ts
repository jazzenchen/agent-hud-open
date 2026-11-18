import { test } from "node:test";
import assert from "node:assert/strict";
import { defaultHUDConfig, simpleNumberColor } from "@agent-hud/hud/model";
import {
  getScreenHUDConfig,
  parseHUDConfigStore,
  updateScreenHUDConfig,
} from "../src/lib/hud-config-store.ts";

const screen = { id: "main", monitorIndex: 0, hasNotch: false };
const second = { ...screen, id: "second", monitorIndex: 1 };
const saved = { version: 3 as const, global: {}, screens: {} };

test("older HUD settings keep level colors at their original opacity", () => {
  const config = getScreenHUDConfig(saved, screen);
  assert.equal(config.greyNumbers, false);
  assert.equal(config.greyNumberOpacity, 1);
  assert.equal(simpleNumberColor(config, true, "#ffd23f"), "#ffd23f");
});

test("grey numbers persist per display and use neutral text in both themes", () => {
  const updated = updateScreenHUDConfig(saved, screen.id, {
    greyNumbers: true,
    greyNumberOpacity: 0.25,
  });
  const restored = parseHUDConfigStore(JSON.parse(JSON.stringify(updated)))!;
  const config = getScreenHUDConfig(restored, screen);
  assert.equal(
    simpleNumberColor(config, true, "#ffd23f"),
    "rgba(64, 64, 64, 0.25)",
  );
  assert.equal(
    simpleNumberColor(config, false, "#ff453a"),
    "rgba(212, 212, 212, 0.25)",
  );
  assert.equal(getScreenHUDConfig(restored, second).greyNumbers, false);

  const colored = updateScreenHUDConfig(restored, screen.id, {
    greyNumbers: false,
  });
  const coloredConfig = getScreenHUDConfig(colored, screen);
  assert.equal(simpleNumberColor(coloredConfig, false, "#ff453a"), "#ff453a");
  assert.equal(coloredConfig.greyNumberOpacity, 0.25);

  const reset = updateScreenHUDConfig(restored, screen.id, {
    greyNumbers: defaultHUDConfig.greyNumbers,
    greyNumberOpacity: defaultHUDConfig.greyNumberOpacity,
  });
  assert.equal(getScreenHUDConfig(reset, screen).greyNumbers, false);
  assert.equal(getScreenHUDConfig(reset, screen).greyNumberOpacity, 1);
});

test("invalid saved opacity falls back to full opacity", () => {
  for (const opacity of [-0.1, 0.2, 1.1, Infinity, "0.5", null]) {
    const store = parseHUDConfigStore({
      ...saved,
      screens: { main: { greyNumbers: true, greyNumberOpacity: opacity } },
    })!;
    assert.equal(getScreenHUDConfig(store, screen).greyNumberOpacity, 1);
  }
});
