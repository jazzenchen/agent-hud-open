/** A quota window, as far as a held reading needs it. */
interface HeldWindow {
  usedPercent: number | null;
  status: string;
  resetAt?: string | null;
  /** The source's fixed window length; absent/null when it does not report one. */
  windowDurationSeconds?: number | null;
}

/** Whether a window's reset has passed: its share has started over, so its
 * reading no longer says how much is used until the next read (Open #90;
 * vendors.rs `reset_passed`). */
export function resetPassed(
  q: { resetAt?: string | null },
  now: number,
): boolean {
  const reset = q.resetAt ? Date.parse(q.resetAt) : NaN;
  return reset <= now;
}

/** A window that reads live now: a number from the last read, and a reset
 * still ahead. */
export function liveWindow(q: HeldWindow, now: number): boolean {
  return q.usedPercent != null && q.status !== "stale" && !resetPassed(q, now);
}

/**
 * The tightest window of an agent's last reading while every window waits on
 * a failed read ("stale"), so a Simple mark keeps drawing it in the stale
 * grey instead of an empty ring. A window whose reset has passed holds
 * nothing, since its share has started over. Null while any window reads
 * live, or when none has a number (vendors.rs `held`).
 */
export function heldUsed(
  quotas: readonly HeldWindow[],
  now: number,
): number | null {
  if (quotas.some((q) => liveWindow(q, now))) return null;
  let held: number | null = null;
  for (const q of quotas) {
    if (q.usedPercent == null || !Number.isFinite(q.usedPercent)) continue;
    if (resetPassed(q, now)) continue;
    if (held == null || q.usedPercent > held) held = q.usedPercent;
  }
  return held;
}

/** The live window with the highest used percent; the first of equal
 * readings stays the tightest (vendors.rs `tightest`). */
export function tightestWindow<T extends HeldWindow>(
  quotas: readonly T[],
  now: number,
): T | undefined {
  return quotas
    .filter((q) => liveWindow(q, now))
    .reduce<T | undefined>(
      (a, b) => (!a || b.usedPercent! > a.usedPercent! ? b : a),
      undefined,
    );
}

/** The live window with the shortest reported window; ties go to the
 * highest used percent. When no live window reports a window length, the
 * tightest window instead, so a screen without duration data reads as it
 * did (vendors.rs `shortest`). */
export function shortestWindow<T extends HeldWindow>(
  quotas: readonly T[],
  now: number,
): T | undefined {
  const timed = quotas.filter(
    (q) =>
      liveWindow(q, now) &&
      q.windowDurationSeconds != null &&
      Number.isFinite(q.windowDurationSeconds) &&
      q.windowDurationSeconds > 0,
  );
  if (!timed.length) return tightestWindow(quotas, now);
  return timed.reduce<T | undefined>(
    (a, b) =>
      !a ||
      b.windowDurationSeconds! < a.windowDurationSeconds! ||
      (b.windowDurationSeconds === a.windowDurationSeconds &&
        b.usedPercent! > a.usedPercent!)
        ? b
        : a,
    undefined,
  );
}

/** What an agent's windows set on its mark: the share of its tightest live
 * window, or of its shortest when the screen asks for that, else the share
 * its last reading holds, in the stale grey. The level, and with it every
 * colour, stays the tightest window's either way, so a spent weekly window
 * still glows while the number shows the 5-hour one. */
export function windowReading<T extends HeldWindow>(
  quotas: readonly T[],
  now: number,
  shortestFirst = false,
): { used: number | null; heldUsed: number | null; level: T["status"] } {
  const tightest = tightestWindow(quotas, now);
  const shown = shortestFirst ? shortestWindow(quotas, now) : tightest;
  return {
    used: shown?.usedPercent ?? null,
    heldUsed: heldUsed(quotas, now),
    level: tightest?.status ?? "stale",
  };
}
