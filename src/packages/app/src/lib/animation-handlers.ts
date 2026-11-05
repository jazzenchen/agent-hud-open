/**
 * WebKit never frees a removed element while one of its animations keeps a
 * handler that reaches back to it: element → animation → onfinish/oncancel →
 * closure → element. motion sets onfinish on every animation it starts and
 * never clears it, so every card and panel it had animated stayed in the HUD's
 * web process, with the data it showed (issue 47). Once an animation has
 * finished or been cancelled its handlers have run or never will (nothing here
 * plays one again), so they are dropped then. App.tsx imports this module, so
 * every window has it before any view animates.
 */
type Handler = (this: Animation, event: AnimationPlaybackEvent) => unknown;

const guarded = new WeakSet<object>();
/** The handler each wrapper stands for, so reading `onfinish` gives back what was set. */
const originals = new WeakMap<Handler, Handler>();

export function releaseAnimationHandlers(
  prototype: Animation | undefined = globalThis.Animation?.prototype,
) {
  if (!prototype || guarded.has(prototype)) return;
  const onfinish = Object.getOwnPropertyDescriptor(prototype, "onfinish");
  const oncancel = Object.getOwnPropertyDescriptor(prototype, "oncancel");
  if (!onfinish?.get || !onfinish.set || !oncancel?.get || !oncancel.set)
    return;
  guarded.add(prototype);
  const drop = (animation: Animation) => {
    onfinish.set!.call(animation, null);
    oncancel.set!.call(animation, null);
  };
  for (const [name, handler] of [
    ["onfinish", onfinish],
    ["oncancel", oncancel],
  ] as const)
    Object.defineProperty(prototype, name, {
      configurable: true,
      enumerable: handler.enumerable,
      get(this: Animation) {
        const set = handler.get!.call(this) as Handler | null;
        return (set && originals.get(set)) ?? set;
      },
      set(this: Animation, value: Handler | null) {
        if (typeof value !== "function") return handler.set!.call(this, value);
        const once = function (this: Animation, event: AnimationPlaybackEvent) {
          drop(this);
          return value.call(this, event);
        };
        originals.set(once, value);
        handler.set!.call(this, once);
      },
    });
  const cancel = prototype.cancel;
  prototype.cancel = function (this: Animation) {
    // A cancelled animation never finishes. It fires `cancel`, whose handler
    // then drops both, unless it was idle already.
    const idle = this.playState === "idle";
    cancel.call(this);
    onfinish.set!.call(this, null);
    if (idle) oncancel.set!.call(this, null);
  };
}

releaseAnimationHandlers();
