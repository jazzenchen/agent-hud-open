import { test } from "node:test";
import assert from "node:assert/strict";
import {
  breathFrames,
  breathTiming,
  glowMotion,
  halfBreath,
  retimedBreath,
} from "../src/glow-motion.ts";
import { glowFrameRate } from "../src/glow-frame-rate.ts";

test("the soft breath is an opacity pulse; patterns that change are drawn frame by frame", () => {
  assert.deepEqual(glowMotion("blur", "breathe", 3, true, true), {
    pulse: true,
    framesPerSecond: 0,
  });
  // GlowWindowController: a running grid effect carries the breathing itself.
  assert.deepEqual(glowMotion("dots", "breathe", 3, true, true), {
    pulse: false,
    framesPerSecond: 24,
  });
  for (const effect of ["flow", "scan", "ripple", "shimmer", "boot"])
    assert.deepEqual(
      glowMotion("blur", effect, 7, true, true),
      { pulse: false, framesPerSecond: 10 },
      effect,
    );
});

test("nothing moves when the rim may not, and without a GPU only the pulse remains", () => {
  assert.deepEqual(glowMotion("blur", "breathe", 3, false, true), {
    pulse: false,
    framesPerSecond: 0,
  });
  assert.deepEqual(glowMotion("ascii", "scan", 3, false, true), {
    pulse: false,
    framesPerSecond: 0,
  });
  assert.deepEqual(glowMotion("blur", "breathe", 3, true, false), {
    pulse: true,
    framesPerSecond: 0,
  });
  assert.deepEqual(glowMotion("ascii", "scan", 3, true, false), {
    pulse: false,
    framesPerSecond: 0,
  });
});

test("frames follow GlowAnimator: 24 a second at the working period, down to 8 at rest", () => {
  assert.equal(glowFrameRate(3), 24);
  assert.equal(glowFrameRate(1), 24);
  assert.equal(glowFrameRate(7), 10);
  assert.equal(glowFrameRate(12), 8);
  assert.equal(glowFrameRate(24), 8);
});

test("the breath dims from the drawn peak to peak · (1 − amplitude)", () => {
  assert.deepEqual(
    breathFrames(0.6).map((frame) => frame.opacity),
    [1, 1 - 0.6],
  );
  assert.deepEqual(
    breathFrames(0).map((frame) => frame.opacity),
    [1, 1],
  );
  assert.deepEqual(
    breathFrames(1.4).map((frame) => frame.opacity),
    [1, 0],
    "depth is at most 1",
  );
  assert.deepEqual(
    breathFrames(-1).map((frame) => frame.opacity),
    [1, 1],
  );
});

test("half a period each way, eased, forever, as Open's autoreversing CABasicAnimation", () => {
  assert.deepEqual(breathTiming(3), {
    duration: 1500,
    direction: "alternate",
    iterations: Infinity,
    easing: "ease-in-out",
  });
  assert.equal(halfBreath(7), 3500);
  assert.equal(halfBreath(0.2), 250, "Open's 0.5 s floor");
});

test("a new period keeps the breath where it was", () => {
  // A quarter of the way down at 3 s stays a quarter of the way down at 7 s.
  assert.equal(retimedBreath(375, 1500, 3500), 875);
  // Halfway back up (second, reversed iteration) stays on the way up.
  assert.equal(retimedBreath(1500 + 750, 1500, 3500), 3500 + 1750);
  // Whole cycles already played do not matter.
  assert.equal(retimedBreath(3 * 3000 + 375, 1500, 3500), 875);
  assert.equal(retimedBreath(Number.NaN, 1500, 3500), 0);
  assert.equal(retimedBreath(100, 0, 3500), 0);
});
