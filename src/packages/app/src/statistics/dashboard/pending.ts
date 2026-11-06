import { useEffect, useState, type CSSProperties } from "react";

/** A section's wait for Core's answer to new filters. */
export interface Pending {
  active: boolean;
  /** How long the last answer took, ms. */
  expected: number | null;
}

/** An answer quicker than this shows no wait at all, ms. */
const QUICK_MS = 160;
/** How long the finished wash takes to fill and fade, ms. */
const DONE_MS = 420;

/** The class and style of a section waiting for new filters: its old
 * figures dim, and a wash fills its card from the left over about as long
 * as the last answer took, then fills up and fades as the answer lands.
 * Core reports no progress, so the wash is that expectation, and it holds
 * near the end when an answer is late. The section waits while any of
 * `pending` does. */
export function usePendingLook(...pending: (Pending | null | undefined)[]) {
  const waiting = pending.filter((entry) => entry?.active);
  const active = waiting.length > 0;
  const expected = Math.max(0, ...waiting.map((entry) => entry!.expected ?? 0));
  const [phase, setPhase] = useState<"idle" | "loading" | "done">("idle");
  useEffect(() => {
    if (active) {
      const timer = window.setTimeout(() => setPhase("loading"), QUICK_MS);
      return () => window.clearTimeout(timer);
    }
    setPhase((value) => (value === "loading" ? "done" : "idle"));
    const timer = window.setTimeout(() => setPhase("idle"), DONE_MS);
    return () => window.clearTimeout(timer);
  }, [active]);
  const duration = Math.min(8_000, Math.max(500, expected * 1.15 - QUICK_MS));
  return {
    className: phase === "idle" ? "stats-block" : `stats-block stats-${phase}`,
    style: { "--stats-expected": `${Math.round(duration)}ms` } as CSSProperties,
  };
}
