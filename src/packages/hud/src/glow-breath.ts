import { useEffect, useRef } from "react";
import {
  breathFrames,
  breathTiming,
  halfBreath,
  retimedBreath,
} from "./glow-motion";

/** The rim's soft breath, on the element it returns: an opacity animation of
 * the drawn rim, which WebKit hands to Core Animation, as Open's
 * applyBreathing does: no page or GPU work per frame. A change of period
 * keeps the glow where it is in its breath; a change of depth only reshapes
 * the curve. */
export function useGlowBreath(
  pulse: boolean,
  drawn: boolean,
  amplitude: number,
  cycle: number,
) {
  const breath = useRef<HTMLDivElement | null>(null);
  const breathing = useRef<{ animation: Animation; amplitude: number } | null>(
    null,
  );
  useEffect(() => {
    const element = breath.current;
    if (!pulse || !element) return;
    const animation = element.animate(
      breathFrames(amplitude),
      breathTiming(cycle),
    );
    breathing.current = { animation, amplitude };
    return () => {
      breathing.current = null;
      animation.cancel();
    };
  }, [pulse, drawn]);
  useEffect(() => {
    const running = breathing.current;
    if (!running || running.amplitude === amplitude) return;
    running.amplitude = amplitude;
    const keyframes = running.animation.effect;
    if (keyframes instanceof KeyframeEffect)
      keyframes.setKeyframes(breathFrames(amplitude));
  }, [amplitude]);
  useEffect(() => {
    const animation = breathing.current?.animation;
    const timing = animation?.effect;
    if (!animation || !timing) return;
    const from = Number(timing.getTiming().duration);
    const to = halfBreath(cycle);
    if (from === to) return;
    const time = Number(animation.currentTime ?? 0);
    timing.updateTiming({ duration: to });
    animation.currentTime = retimedBreath(time, from, to);
  }, [cycle]);
  return breath;
}
