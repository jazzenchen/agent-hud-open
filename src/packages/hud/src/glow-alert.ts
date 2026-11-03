// An island alert's flash on the glow rim, as agent-hud-native-open's
// GlowWindowController.applyAlert draws it. Pure, so it can be tested apart
// from the renderer.

export interface GlowAlert {
  id: string;
  color: string;
  warning: boolean;
  /** The sender's part of the gradient and the feather at its ends; null
   * flashes the whole rim. */
  span: { from: number; to: number; feather: number } | null;
}

/** Open's IslandAlert accents: a finished turn is blue, an exhausted quota or
 * a waiting request is a warning, everything else calm. The sender's span is
 * its place among the glow's vendors, feathered by 0.15 / their count. */
export function glowAlert(
  alert: {
    id: string;
    kind:
      | "completion"
      | "quota-exhaustion"
      | "quota-reset"
      | "reset-credits"
      | "sign-in-needed"
      | "request";
    vendor: string;
  } | null,
  glowVendors: string[],
): GlowAlert | null {
  if (!alert) return null;
  const warning =
    alert.kind === "quota-exhaustion" ||
    alert.kind === "sign-in-needed" ||
    alert.kind === "request";
  const color =
    alert.kind === "completion" ? "#82aaff" : warning ? "#e9a16d" : "#6cd8ac";
  const first = glowVendors.indexOf(alert.vendor);
  const last = glowVendors.lastIndexOf(alert.vendor);
  const count = glowVendors.length;
  return {
    id: alert.id,
    color,
    warning,
    span:
      first < 0
        ? null
        : {
            from: first / count,
            to: (last + 1) / count,
            feather: 0.15 / count,
          },
  };
}

/** Open's pulse over 1.8 s: a double flash for a warning, one for news, a
 * faint one under Reduce Motion. */
export function pulseFrames(warning: boolean, reduced: boolean): Keyframe[] {
  if (reduced)
    return [{ opacity: 0 }, { opacity: 0.12, offset: 0.3 }, { opacity: 0 }];
  return warning
    ? [
        { opacity: 0 },
        { opacity: 0.22, offset: 0.2 },
        { opacity: 0.05, offset: 0.45 },
        { opacity: 0.22, offset: 0.65 },
        { opacity: 0 },
      ]
    : [
        { opacity: 0 },
        { opacity: 0.22, offset: 0.25 },
        { opacity: 0.15, offset: 0.65 },
        { opacity: 0 },
      ];
}

/** The mask that keeps the flash to the sender's span of a rim `width` wide
 * drawn inside `padding`; undefined for the whole rim. */
export function flashMask(
  span: GlowAlert["span"],
  width: number,
  padding: number,
): string | undefined {
  if (!span) return undefined;
  const total = width + padding * 2;
  const at = (fraction: number) =>
    `${+(((padding + fraction * width) / total) * 100).toFixed(4)}%`;
  const { from, to, feather } = span;
  return `linear-gradient(to right, transparent ${at(Math.max(0, from - feather))}, black ${at(from + feather)}, black ${at(to - feather)}, transparent ${at(Math.min(1, to + feather))})`;
}
