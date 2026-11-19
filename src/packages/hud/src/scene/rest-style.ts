// How a resting Simple dock draws: CSS only, changed by transitions so each
// fold and unfold plays without a script animation, at once where motion is
// reduced. Pure, so the tests run it in Node.
import type { CSSProperties } from "react";
import type { StripMetrics } from "../geometry.ts";
import type { HUDEdge } from "../model.ts";
import { REST_MS, type Rest } from "./rest.ts";

const EASE = "cubic-bezier(0.2, 0.8, 0.2, 1)";

/** A change of `properties` over the fold's time; none where motion is
 * reduced, so the dock folds and unfolds at once. */
export function restTransition(
  properties: readonly string[],
  reduceMotion: boolean,
): string | undefined {
  if (reduceMotion) return undefined;
  return properties.map((name) => `${name} ${REST_MS}ms ${EASE}`).join(", ");
}

/** Corners rounded only on the side away from `edge`: a tab flush with it.
 * In border-radius order: top left, top right, bottom right, bottom left. */
export function innerCorners(edge: HUDEdge, radius: number): string {
  const r = `${Math.max(0, radius)}px`;
  switch (edge) {
    case "top":
      return `0 0 ${r} ${r}`;
    case "bottom":
      return `${r} ${r} 0 0`;
    case "left":
      return `0 ${r} ${r} 0`;
    case "right":
      return `${r} 0 0 ${r}`;
  }
}

/** Moved `by` points into `edge`. */
export function intoEdge(edge: HUDEdge, by: number): string {
  switch (edge) {
    case "top":
      return `translateY(${-by}px)`;
    case "bottom":
      return `translateY(${by}px)`;
    case "left":
      return `translateX(${-by}px)`;
    case "right":
      return `translateX(${by}px)`;
  }
}

/** How the dock's own strip draws over its usual style: shrunk to its
 * logos' strip flush against the edge, or faded back into the edge behind
 * a bar or nothing. Nothing at all for a dock that never folds, and only
 * the transition while it is out. */
export function restingDockStyle(rest: Rest | undefined): CSSProperties {
  if (!rest) return {};
  const { style, folded, edge, shape, logos, reduceMotion } = rest;
  if (style === "logos") {
    const transition = restTransition(
      ["width", "height", "padding", "gap", "margin", "border-radius"],
      reduceMotion,
    );
    if (!folded) return { transition };
    return {
      width: shape.width,
      height: shape.height,
      marginTop: 0,
      marginRight: 0,
      marginBottom: 0,
      marginLeft: 0,
      paddingTop: logos.pad,
      paddingRight: logos.pad,
      paddingBottom: logos.pad,
      paddingLeft: logos.pad,
      gap: logos.gap,
      borderRadius: innerCorners(edge, logos.radius),
      transition,
    };
  }
  const transition = restTransition(
    ["opacity", "transform", "visibility"],
    reduceMotion,
  );
  if (!folded) return { transition };
  return {
    opacity: 0,
    transform: intoEdge(edge, 8),
    visibility: "hidden",
    transition,
  };
}

/** A resting strip held against its edge rather than centred in its box, so
 * a dock that shrinks to its logos stays flush with the edge. */
export function flushWith(edge: HUDEdge): CSSProperties {
  switch (edge) {
    case "top":
      return { alignItems: "flex-start" };
    case "bottom":
      return { alignItems: "flex-end" };
    case "left":
      return { justifyContent: "flex-start" };
    case "right":
      return { justifyContent: "flex-end" };
  }
}

/** A mark's parts while its dock rests as its logos (`compact`) and while it
 * is out: the logo stays as it is, while the ring fades into it and the
 * bars and figures fold away to nothing. */
export function compactMark(
  compact: boolean,
  metrics: Pick<
    StripMetrics,
    "mark" | "logo" | "labelGap" | "bar" | "label" | "font"
  >,
  reduceMotion: boolean,
): Record<"button" | "mark" | "ring" | "figures", CSSProperties> {
  const motion = (properties: string[]) =>
    restTransition(properties, reduceMotion);
  const side = compact ? metrics.logo : metrics.mark;
  return {
    button: {
      gap: compact ? 0 : metrics.labelGap,
      transition: motion(["gap"]),
    },
    mark: {
      width: side,
      height: side,
      transition: motion(["width", "height"]),
    },
    ring: {
      position: "absolute",
      left: "50%",
      top: "50%",
      opacity: compact ? 0 : 1,
      transform: `translate(-50%, -50%) scale(${compact ? 0.5 : 1})`,
      transition: motion(["opacity", "transform"]),
    },
    figures: {
      display: "inline-flex",
      alignItems: "center",
      gap: metrics.labelGap,
      flexShrink: 0,
      overflow: "hidden",
      maxWidth: compact
        ? 0
        : metrics.bar + metrics.labelGap + metrics.label + metrics.font,
      maxHeight: compact ? 0 : Math.max(metrics.mark, metrics.font * 2),
      opacity: compact ? 0 : 1,
      transition: motion(["max-width", "max-height", "opacity"]),
    },
  };
}
