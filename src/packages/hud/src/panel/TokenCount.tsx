import { useEffect } from "react";
import {
  animate,
  motion,
  useMotionValue,
  useTransform,
  type Transition,
} from "motion/react";
import { tokensShort } from "@agent-hud/ui/format";

/** A total of tokens, counting to each new value over `moving`. */
export function TokenCount({
  tokens,
  still,
  moving,
}: {
  tokens: number;
  still: boolean;
  moving: Transition;
}) {
  const count = useMotionValue(tokens);
  const text = useTransform(count, (value) => tokensShort(Math.round(value)));
  useEffect(() => {
    if (still) {
      count.jump(tokens);
      return;
    }
    const controls = animate(count, tokens, moving);
    return () => controls.stop();
  }, [count, tokens, still, moving]);
  return still ? tokensShort(tokens) : <motion.span>{text}</motion.span>;
}
