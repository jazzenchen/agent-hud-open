/* A column's bar in the panel's token chart: its bucket's tokens, stacked by
 * model in each model's agent colour, growing in as its range comes and
 * shrinking away as it goes. */
import { motion, type Transition } from "motion/react";
import { HUD_METRICS } from "../model";
import { agentColor } from "./theme";

export function TokenBar({
  height,
  models,
  stacked,
  still,
  faded,
  fill,
  light,
  transition,
}: {
  /** Its share of the chart's height, as a percentage. */
  height: string;
  /** The bucket's tokens by model, when the answer gives them. */
  models?: Record<string, number>;
  /** Models with usage somewhere in the range, each in its colour slot. */
  stacked: { model: { id: string }; slot: number }[];
  /** Drawn at its height at once, with no frame to wait for. */
  still: boolean;
  /** Dimmed while another column is pointed at. */
  faded: boolean;
  /** The bar's colour when it has no models. */
  fill: string;
  light: boolean;
  transition: Transition;
}) {
  return (
    <motion.div
      initial={still ? false : { height: "0%" }}
      animate={still ? undefined : { height }}
      exit={still ? undefined : { height: "0%" }}
      transition={transition}
      style={{
        height: still ? height : undefined,
        width: "100%",
        display: "flex",
        flexDirection: "column-reverse",
        borderRadius: "2px 2px 0 0",
        overflow: "hidden",
        opacity: faded ? 0.55 : 1,
        background: models ? "transparent" : fill,
      }}
    >
      {models &&
        stacked.map(({ model, slot }) => (
          <i
            key={model.id}
            style={{
              display: "block",
              flex: `${models[model.id] ?? 0} 0 0`,
              background: agentColor(slot, light),
              transition: still
                ? undefined
                : `flex-grow ${HUD_METRICS.duration}s cubic-bezier(${HUD_METRICS.ease.join(",")})`,
            }}
          />
        ))}
    </motion.div>
  );
}
