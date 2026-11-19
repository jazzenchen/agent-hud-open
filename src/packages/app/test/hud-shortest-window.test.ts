import { test } from "node:test";
import assert from "node:assert/strict";
import { defaultHUDConfig } from "@agent-hud/hud/model";
import {
  getScreenHUDConfig,
  parseHUDConfigStore,
  updateScreenHUDConfig,
} from "../src/lib/hud-config-store.ts";

const screen = { id: "main", monitorIndex: 0, hasNotch: false };
const second = { ...screen, id: "second", monitorIndex: 1 };
const saved = { version: 3 as const, global: {}, screens: {} };

test("older HUD settings keep the tightest window as the number", () => {
  const config = getScreenHUDConfig(saved, screen);
  assert.equal(config.shortestWindowFirst, false);
});

test("shortest window first persists per display", () => {
  const updated = updateScreenHUDConfig(saved, screen.id, {
    shortestWindowFirst: true,
  });
  const restored = parseHUDConfigStore(JSON.parse(JSON.stringify(updated)))!;
  assert.equal(getScreenHUDConfig(restored, screen).shortestWindowFirst, true);
  assert.equal(getScreenHUDConfig(restored, second).shortestWindowFirst, false);

  const off = updateScreenHUDConfig(restored, screen.id, {
    shortestWindowFirst: false,
  });
  assert.equal(getScreenHUDConfig(off, screen).shortestWindowFirst, false);

  const reset = updateScreenHUDConfig(restored, screen.id, {
    shortestWindowFirst: defaultHUDConfig.shortestWindowFirst,
  });
  assert.equal(getScreenHUDConfig(reset, screen).shortestWindowFirst, false);
});

test("a saved non-boolean falls back to the tightest window", () => {
  for (const value of ["true", 1, null]) {
    const store = parseHUDConfigStore({
      ...saved,
      screens: { main: { shortestWindowFirst: value } },
    })!;
    assert.equal(getScreenHUDConfig(store, screen).shortestWindowFirst, false);
  }
});
