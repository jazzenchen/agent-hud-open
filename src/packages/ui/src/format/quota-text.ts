// Quota readings and outlook in words.
import { forecast } from "./durations.ts";
import { translate, type Language } from "./language.ts";
import { roundHalfAway } from "./numbers.ts";

export type QuotaDisplay = "consumed" | "remaining";

/** Presentation only: unknown readings stay unknown and source usage stays unchanged. */
export function quotaPercent(
  used: number | null | undefined,
  display: QuotaDisplay = "consumed",
): number | null {
  if (used == null || !Number.isFinite(used)) return null;
  const bounded = Math.max(0, Math.min(100, used));
  return display === "remaining" ? 100 - bounded : bounded;
}

export function quotaPercentLabel(
  used: number | null | undefined,
  display: QuotaDisplay = "consumed",
): string {
  const value = quotaPercent(used, display);
  return value == null ? "—" : `${Math.round(value)}%`;
}

export function quotaDisplayLabel(
  display: QuotaDisplay = "consumed",
  language: Language = "en",
): string {
  return display === "remaining"
    ? translate(language, "Quota remaining")
    : translate(language, "Quota used");
}

export type OutlookKind =
  | "untimed"
  | "exhausted"
  | "no-estimate"
  | "no-usage"
  | "insufficient-data"
  | "exhausts";

/** The share of the window used by its reset: "85% by reset" / "重置时 85%". */
export function byReset(projectedUsedPercent: number, language: Language) {
  const value = roundHalfAway(projectedUsedPercent);
  return translate(language, "{{value}}% by reset", { value });
}

/**
 * A window's outlook in words: nothing for a window without a reset, how long the rest lasts for one that runs out
 * before its reset, and the share its pace uses by the reset for one that runs out after it.
 */
export function outlookText(
  outlook: { kind: OutlookKind; seconds?: number; beforeReset?: boolean },
  projectedUsedAtReset: number | null,
  language: Language,
): string | null {
  switch (outlook.kind) {
    case "untimed":
      return null;
    case "exhausted":
      return translate(language, "Exhausted");
    case "no-estimate":
      return translate(language, "No estimate");
    case "no-usage":
      return translate(language, "No usage");
    case "insufficient-data":
      return translate(language, "Insufficient data");
    case "exhausts": {
      if (outlook.beforeReset === false) {
        return projectedUsedAtReset === null
          ? translate(language, "Insufficient data")
          : byReset(projectedUsedAtReset, language);
      }
      const exhaustion = forecast(outlook.seconds ?? 0, language);
      return translate(language, "Exhausts ~{{exhaustion}}", { exhaustion });
    }
  }
}

/** What a forecast's pace comes from: this cycle's recent readings, or the cycle so far on a straight line. */
export function outlookBasisText(
  basis: "recent" | "window",
  language: Language,
) {
  return basis === "window"
    ? translate(language, "Estimated from the whole window")
    : translate(language, "At the recent pace");
}

/** Where a window's use stands against a steady pace through its cycle: "12 pts ahead of budget" / "比预算超前 12 个点". */
export function budgetText(
  budget: { paceDelta: number; status: "ahead" | "behind" | "onTrack" },
  language: Language,
) {
  const points = roundHalfAway(Math.abs(budget.paceDelta));
  switch (budget.status) {
    case "ahead":
      return translate(language, "{{count}} pts ahead of budget", {
        count: points,
      });
    case "behind":
      return translate(language, "{{count}} pts under budget", {
        count: points,
      });
    case "onTrack":
      return translate(language, "On budget");
  }
}

/** The burn rate's detail while readings are still too few: first sampling or again after a reset. */
export function samplingLabel(afterReset: boolean, language: Language) {
  return afterReset
    ? translate(language, "Resampling")
    : translate(language, "Sampling");
}

/** The sampling hint: why, then the span observed so far beside the span needed. */
export function samplingHint(
  elapsed: number,
  required: number,
  afterReset: boolean,
  language: Language,
) {
  const reason = afterReset
    ? translate(language, "Quota reset; collecting new readings")
    : translate(language, "Collecting initial readings for a forecast");
  // A partial minute must not claim the minimum has been met.
  const observed =
    elapsed < 60
      ? translate(language, "less than 1m")
      : forecast(Math.floor(elapsed / 60) * 60, language);
  const minimum = forecast(required, language);
  return `${reason}\n${translate(language, "Sampled {{observed}}; at least {{minimum}} of readings needed", { observed, minimum })}`;
}
