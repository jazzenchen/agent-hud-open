/* What a Simple mark draws round and beside its logo: rings, a segmented
 * ring or bars. The HUD strip and Settings' preview both draw marks with
 * these. */
import type { PanelLayout } from "@agent-hud/protocol";
import { quotaPercent, type QuotaDisplay } from "@agent-hud/ui/format";
import { statusColor, type HUDVendor } from "../model";
import { simpleRingSpec, type RingStyle } from "./arcs";
import {
  markTrack,
  markWindows,
  resetPassed,
  type RingItem,
} from "./mark-windows";
import { UsageBars } from "./UsageBars";
import { UsageRing } from "./UsageRing";

export { markTrack, markWindows, type RingItem } from "./mark-windows";

/** Each window as a ring draws it: its share as the panel shows shares, in
 * its level's colour; a held window whose reset has passed starts over at 0. */
export function markItems(
  vendor: Pick<HUDVendor, "quotas">,
  display: QuotaDisplay | undefined,
  layout?: PanelLayout,
  now = Date.now(),
): RingItem[] {
  return markWindows(vendor, layout, now).map((q) => ({
    percent:
      q.status === "stale" && resetPassed(q, now)
        ? 0
        : quotaPercent(q.usedPercent, display),
    color: statusColor(q.status),
  }));
}

/** A Simple mark's ring, filling the mark round its logo. */
export function MarkRing({
  style,
  items,
  size,
  ringWidth,
  light,
  reduceMotion,
}: {
  style: RingStyle;
  items: RingItem[];
  size: number;
  ringWidth: number;
  light: boolean;
  reduceMotion?: boolean;
}) {
  return (
    <UsageRing
      style={style}
      items={items}
      spec={simpleRingSpec(size, ringWidth)}
      track={markTrack(light)}
      reduceMotion={reduceMotion}
      styles={{ position: "absolute", inset: 0 }}
    />
  );
}

/** A Simple mark's bars beside its logo: 1.09 marks long, half a point thicker
 * than its ring would be, 1.5 pt apart at 22 pt. */
export function MarkBars({
  items,
  length,
  height,
  size,
  ringWidth,
  light,
  reduceMotion,
}: {
  items: RingItem[];
  length: number;
  height: number;
  size: number;
  ringWidth: number;
  light: boolean;
  reduceMotion?: boolean;
}) {
  return (
    <UsageBars
      items={items}
      length={length}
      height={height}
      thickness={ringWidth + 0.5}
      gap={Math.max(1, size * 0.068)}
      track={markTrack(light)}
      reduceMotion={reduceMotion}
    />
  );
}
