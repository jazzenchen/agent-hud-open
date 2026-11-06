import { test } from "node:test";
import assert from "node:assert/strict";
import {
  afterFlash,
  flashAnimation,
  flashKeyframes,
  flashMask,
  glowAlert,
  pulseFrames,
} from "../src/glow-alert.ts";

test("accents follow Open's IslandAlert: blue turns, warm warnings, calm news", () => {
  const vendors = ["claude", "codex", "kimi"];
  assert.equal(
    glowAlert({ id: "a", kind: "completion", vendor: "claude" }, vendors)
      ?.color,
    "#82aaff",
  );
  assert.deepEqual(
    glowAlert({ id: "b", kind: "quota-exhaustion", vendor: "codex" }, vendors),
    {
      id: "b",
      color: "#e9a16d",
      warning: true,
      span: { from: 1 / 3, to: 2 / 3, feather: 0.15 / 3 },
    },
  );
  assert.equal(
    glowAlert({ id: "c", kind: "request", vendor: "kimi" }, vendors)?.warning,
    true,
  );
  assert.equal(
    glowAlert({ id: "d", kind: "quota-reset", vendor: "kimi" }, vendors)?.color,
    "#6cd8ac",
  );
  assert.equal(
    glowAlert({ id: "e", kind: "reset-credits", vendor: "grok" }, vendors)
      ?.span,
    null,
    "a sender without a colour flashes the whole rim",
  );
  assert.equal(
    glowAlert({ id: "f", kind: "sign-in-needed", vendor: "claude" }, vendors)
      ?.warning,
    true,
    "a signed-out agent is a warning",
  );
  assert.equal(
    glowAlert(
      { id: "g", kind: "hooks-review-needed", vendor: "codex" },
      vendors,
    )?.warning,
    true,
    "an agent skipping Agent HUD's hooks is a warning",
  );
  assert.equal(glowAlert(null, vendors), null);
});

test("a warning flashes twice, news once, Reduce Motion faintly", () => {
  assert.deepEqual(
    pulseFrames(true, false).map((frame) => frame.opacity),
    [0, 0.22, 0.05, 0.22, 0],
  );
  assert.deepEqual(
    pulseFrames(false, false).map((frame) => frame.opacity),
    [0, 0.22, 0.15, 0],
  );
  assert.deepEqual(
    pulseFrames(true, true).map((frame) => frame.opacity),
    [0, 0.12, 0],
  );
});

test("the mask keeps the flash to the sender's span inside the padding", () => {
  // A 100-wide rim inside 50 of padding on each side: the rim runs 25%–75%.
  assert.equal(
    flashMask({ from: 0.5, to: 1, feather: 0.075 }, 100, 50),
    "linear-gradient(to right, transparent 46.25%, black 53.75%, black 71.25%, transparent 75%)",
  );
  assert.equal(flashMask(null, 100, 50), undefined);
});

test("a flash keeps to its sender's part of a rim moving inside a wider box", () => {
  // The same 100-wide rim, 100 into a 300-wide box: 200 of 400 is the rim's start.
  assert.equal(
    flashMask({ from: 0.5, to: 1, feather: 0.075 }, 100, 50, 300, 100),
    "linear-gradient(to right, transparent 48.125%, black 51.875%, black 60.625%, transparent 62.5%)",
  );
});

test("the flash's CSS keyframes are pulseFrames, stop for stop", () => {
  const rules = flashKeyframes().split("\n");
  assert.equal(rules.length, 3);
  for (const [warning, reduced] of [
    [false, false],
    [true, false],
    [false, true],
  ] as const) {
    const name = flashAnimation(warning, reduced).split(" ")[0];
    const rule = rules.find((r) => r.startsWith(`@keyframes ${name} `));
    assert.ok(rule, name);
    const stops = [
      ...rule!.matchAll(/([\d.]+)% \{ opacity: ([\d.]+); \}/g),
    ].map(([, at, opacity]) => [Number(at), Number(opacity)]);
    const frames = pulseFrames(warning, reduced);
    assert.deepEqual(
      stops,
      frames.map((frame, index) => [
        (frame.offset ?? (index === 0 ? 0 : 1)) * 100,
        frame.opacity,
      ]),
    );
  }
  assert.equal(
    flashAnimation(true, false),
    "hud-glow-flash-warning 1800ms linear forwards",
  );
});

test("a flash ends unless a newer alert has taken its place", () => {
  const first = glowAlert({ id: "a", kind: "completion", vendor: "claude" }, [
    "claude",
  ])!;
  const second = glowAlert({ id: "b", kind: "completion", vendor: "claude" }, [
    "claude",
  ])!;
  assert.equal(afterFlash(first, "a"), null);
  assert.equal(afterFlash(second, "a"), second);
  assert.equal(afterFlash(null, "a"), null);
});
