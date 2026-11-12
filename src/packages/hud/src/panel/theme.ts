import type { ComponentType, CSSProperties } from "react";
import { DollarSign, Flame, Gauge, Percent, Sigma } from "lucide-react";
import type { QuotaMeasure } from "@agent-hud/protocol";
import { hudPalette, type Ground } from "../tokens";

export type Logo = ComponentType<{
  source: string;
  size?: number;
  light?: boolean;
}>;
export type Measure = QuotaMeasure;
export type Period = "24h" | "7d" | "30d";

/** Source: IslandRowLayout, PanelSpacingMetrics(.regular), HoverPanelView. */
export const SOURCE_PANEL = {
  inset: 6,
  marker: 8,
  rowGap: 12,
  nameWidth: 152,
  valueWidth: 62,
  detailWidth: 96,
  sections: 10,
  blocks: 10,
  divided: 8,
  blockParts: 8,
  rowPadding: 4,
  sessionRows: 6,
  headingSize: 13,
  accountSize: 11,
  rowSize: 13,
  detailSize: 12,
  headingHeight: 18,
  headingVerticalPadding: 1,
  footerSpacing: 10,
  footerTop: 6,
  footerButton: 28,
  compactChartHeight: 40,
} as const;

export const measures: { key: Measure; title: string; icon: typeof Percent }[] =
  [
    { key: "quota", title: "Quota", icon: Percent },
    { key: "burn", title: "Burn rate", icon: Flame },
    { key: "tokens", title: "Reply rate", icon: Gauge },
    { key: "consumed", title: "Consumed", icon: Sigma },
    { key: "cost", title: "Est. cost", icon: DollarSign },
  ];

export const noButtonChrome: CSSProperties = {
  border: 0,
  background: "none",
  color: "inherit",
  font: "inherit",
  padding: 0,
  borderRadius: 0,
  boxShadow: "none",
  textAlign: "left",
};

/** A block's own ground and frame, as a bento's cell has: a tint of the ink
 * under it and a hairline around it. */
export function card(colors: ReturnType<typeof hudPalette>): CSSProperties {
  return {
    background: colors.card,
    boxShadow: `inset 0 0 0 1px ${colors.divider}`,
    borderRadius: 10,
    padding: "8px 10px",
  };
}

export const tabular: CSSProperties = { fontVariantNumeric: "tabular-nums" };

/** Unknown or newly discovered blocks retain their source order after saved ones. */
export function inPanelOrder<T extends { key: string }>(
  items: T[],
  order: readonly string[],
) {
  const positions = new Map(order.map((key, index) => [key, index]));
  return [...items].sort(
    (a, b) =>
      (positions.get(a.key) ?? Infinity) - (positions.get(b.key) ?? Infinity),
  );
}

export const densityMetrics = {
  // PanelSpacingMetrics.swift: roomy / regular / compact. Fonts and chart stay fixed.
  comfortable: { section: 14, block: 11, divided: 11, row: 6, session: 8 },
  normal: { section: 10, block: 8, divided: 8, row: 4, session: 6 },
  compact: { section: 6, block: 5, divided: 5, row: 2, session: 4 },
} as const;

/** One agent's colour by its palette index (Open's AgentPalette): the
 * statistics palette on a dark HUD, each hue's deeper shade on a light one,
 * which stands at least 3:1 there as a dot or a bar must. */
const AGENT_DARK = [
  "#c084fc",
  "#60a5fa",
  "#2dd4bf",
  "#fb923c",
  "#f472b6",
  "#a3e635",
];
const AGENT_LIGHT = [
  "#9333ea",
  "#2563eb",
  "#0d9488",
  "#ea580c",
  "#db2777",
  "#4d7c0f",
];
export function agentColor(index: number, light: boolean) {
  const colors = light ? AGENT_LIGHT : AGENT_DARK;
  return colors[((index % colors.length) + colors.length) % colors.length];
}

/** The panel's tokens on `ground`: the light or dark HUD's, opaque or on a
 * ground the desktop shows through (`hud/tokens.ts`, Open's Theme.under). */
export function palette(ground: Ground) {
  return hudPalette(ground);
}
