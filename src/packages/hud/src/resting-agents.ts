// Which agents a resting HUD shows, by mode, before a layout's own list
// narrows them. Kept free of the UI so the tests can run it.
import type { Snapshot } from "@agent-hud/protocol";
import { agentArtworkKey as vendorKey } from "./agent-artwork.ts";

type HUDMode = "notch" | "simple";

/** Acquisition identity stays distinct from artwork (Codex shares its OpenAI mark). */
export function sourceID(snapshot: Snapshot, source: string): string {
  const normalize = (value: string) =>
    value.trim().toLowerCase().replace(/[ _]+/g, "-");
  const key = normalize(source);
  return (
    snapshot.providers.find(
      (provider) =>
        normalize(provider.id) === key || normalize(provider.name) === key,
    )?.id ?? key
  );
}

/** The agents a resting HUD in `mode` shows without a list, one per mark, in
 * Open's order (ScreenAgents.available):
 * - notch, the glow's parts: each window whose reading shows a level, then
 *   each balance (ReportView.glowSegments);
 * - Simple: every window's agent in row order, then the agents billed by
 *   balance alone (SimpleMeterItem.items).
 * Each mode also includes detected, enabled clients without a reading. */
export function restingAgents(snapshot: Snapshot, mode: HUDMode): string[] {
  const order: string[] = [];
  const add = (source: string) => {
    if (!order.some((s) => vendorKey(s) === vendorKey(source)))
      order.push(source);
  };
  snapshot.quotas
    .filter(
      (q) =>
        mode === "simple" || (q.usedPercent != null && q.status !== "stale"),
    )
    .forEach((q) => add(q.vendor));
  snapshot.billing.forEach((b) => add(b.vendor));
  snapshot.providers
    .filter((provider) => provider.detected && provider.enabled)
    .forEach((provider) => add(provider.id));
  return order;
}

/** The marks a layout shows (HUDAgentOrder.swift, vendors.rs `configured`),
 * in Settings' agent order, so the panel and the HUD share one order: the
 * panel's blocks, a balance block standing for its agent; an agent with no
 * block keeps its place after them (Open #31). The layout's own `hud.agents`
 * list picks which marks show but not their order; one naming none of them
 * never empties a HUD. */
export function layoutMarks<T extends { key: string }>(
  available: T[],
  layout: { order?: string[]; hud?: { agents?: string[] } },
): T[] {
  const marks = (layout.hud?.agents ?? []).map(vendorKey);
  const named = available.filter((vendor) => marks.includes(vendor.key));
  const order = (layout.order ?? []).map((key) =>
    vendorKey(key.replace(/^api:/, "")),
  );
  const rank = (vendor: T) =>
    order.includes(vendor.key) ? order.indexOf(vendor.key) : order.length;
  return (named.length ? named : [...available]).sort(
    (a, b) => rank(a) - rank(b),
  );
}
