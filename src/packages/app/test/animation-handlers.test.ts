import { test } from "node:test";
import assert from "node:assert/strict";
import { releaseAnimationHandlers } from "../src/lib/animation-handlers.ts";

type Handler = ((this: unknown, event: { type: string }) => unknown) | null;

/** Enough of Animation: handler attributes and when its events fire. */
class FakeAnimation {
  playState = "running";
  #finish: Handler = null;
  #cancel: Handler = null;
  get onfinish() {
    return this.#finish;
  }
  set onfinish(handler: Handler) {
    this.#finish = handler;
  }
  get oncancel() {
    return this.#cancel;
  }
  set oncancel(handler: Handler) {
    this.#cancel = handler;
  }
  /** What the animation itself holds, past the guard's getters. */
  held() {
    return { finish: this.#finish, cancel: this.#cancel };
  }
  finish() {
    this.playState = "finished";
    this.#finish?.call(this, { type: "finish" });
  }
  cancel() {
    // The cancel event is queued, and none fires for an idle animation.
    const idle = this.playState === "idle";
    this.playState = "idle";
    if (!idle)
      queueMicrotask(() => this.#cancel?.call(this, { type: "cancel" }));
  }
}
releaseAnimationHandlers(FakeAnimation.prototype as unknown as Animation);
releaseAnimationHandlers(FakeAnimation.prototype as unknown as Animation);

test("a finished animation runs its finish handler once, then holds no handler", () => {
  const animation = new FakeAnimation();
  const calls: string[] = [];
  animation.onfinish = () => calls.push("finish");
  animation.oncancel = () => calls.push("cancel");
  animation.finish();
  assert.deepEqual(calls, ["finish"]);
  assert.deepEqual(animation.held(), { finish: null, cancel: null });
});

test("reading a handler gives back the one that was set", () => {
  const animation = new FakeAnimation();
  const handler = () => {};
  animation.onfinish = handler;
  assert.equal(animation.onfinish, handler);
  animation.onfinish = null;
  assert.equal(animation.onfinish, null);
});

test("a cancelled animation drops its finish handler at once and its cancel handler after it runs", async () => {
  const animation = new FakeAnimation();
  const calls: string[] = [];
  animation.onfinish = () => calls.push("finish");
  animation.oncancel = () => calls.push("cancel");
  animation.cancel();
  assert.equal(animation.held().finish, null);
  assert.notEqual(animation.held().cancel, null);
  await Promise.resolve();
  assert.deepEqual(calls, ["cancel"]);
  assert.deepEqual(animation.held(), { finish: null, cancel: null });
});

test("cancelling an idle animation, which fires nothing, drops both handlers", () => {
  const animation = new FakeAnimation();
  animation.playState = "idle";
  animation.onfinish = () => {};
  animation.oncancel = () => {};
  animation.cancel();
  assert.deepEqual(animation.held(), { finish: null, cancel: null });
});
