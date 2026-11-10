// Quota readings and outlook in words.
import { forecast } from "./durations.ts";
import { text, type Language } from "./language.ts";
import { roundHalfAway } from "./numbers.ts";

export type QuotaDisplay = "consumed" | "remaining";

/** Presentation only: unknown readings stay unknown and source usage stays unchanged. */
export function quotaPercent(
  used: number | null | undefined,
  display: QuotaDisplay = "remaining",
): number | null {
  if (used == null || !Number.isFinite(used)) return null;
  const bounded = Math.max(0, Math.min(100, used));
  return display === "remaining" ? 100 - bounded : bounded;
}

export function quotaPercentLabel(
  used: number | null | undefined,
  display: QuotaDisplay = "remaining",
): string {
  const value = quotaPercent(used, display);
  return value == null ? "—" : `${Math.round(value)}%`;
}

export function quotaDisplayLabel(
  display: QuotaDisplay = "remaining",
  language: Language = "en",
): string {
  return display === "remaining"
    ? language === "zh"
      ? "剩余额度"
      : "Quota remaining"
    : language === "zh"
      ? "已用额度"
      : "Quota used";
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
  return text(language, `重置时 ${value}%`, `${value}% by reset`);
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
      return text(language, "已耗尽", "Exhausted");
    case "no-estimate":
      return text(language, "暂无预测", "No estimate");
    case "no-usage":
      return text(language, "暂无消耗", "No usage");
    case "insufficient-data":
      return text(language, "记录不足", "Insufficient data");
    case "exhausts": {
      if (outlook.beforeReset === false) {
        return projectedUsedAtReset === null
          ? text(language, "记录不足", "Insufficient data")
          : byReset(projectedUsedAtReset, language);
      }
      const exhaustion = forecast(outlook.seconds ?? 0, language);
      return text(language, `耗尽 ~${exhaustion}`, `Exhausts ~${exhaustion}`);
    }
  }
}

/** What a forecast's pace comes from: this cycle's recent readings, or the cycle so far on a straight line. */
export function outlookBasisText(
  basis: "recent" | "window",
  language: Language,
) {
  return basis === "window"
    ? text(language, "按整个窗口估算", "Estimated from the whole window")
    : text(language, "按目前节奏", "At the recent pace");
}

/** Where a window's use stands against a steady pace through its cycle: "12 pts ahead of budget" / "比预算超前 12 个点". */
export function budgetText(
  budget: { paceDelta: number; status: "ahead" | "behind" | "onTrack" },
  language: Language,
) {
  const points = roundHalfAway(Math.abs(budget.paceDelta));
  const unit = points === 1 ? "pt" : "pts";
  switch (budget.status) {
    case "ahead":
      return text(
        language,
        `比预算超前 ${points} 个点`,
        `${points} ${unit} ahead of budget`,
      );
    case "behind":
      return text(
        language,
        `比预算少用 ${points} 个点`,
        `${points} ${unit} under budget`,
      );
    case "onTrack":
      return text(language, "与预算持平", "On budget");
  }
}

/** The burn rate's detail while readings are still too few: first sampling or again after a reset. */
export function samplingLabel(afterReset: boolean, language: Language) {
  return afterReset
    ? text(language, "重新采样", "Resampling")
    : text(language, "采样中", "Sampling");
}

/** The sampling hint: why, then the span observed so far beside the span needed. */
export function samplingHint(
  elapsed: number,
  required: number,
  afterReset: boolean,
  language: Language,
) {
  const reason = afterReset
    ? text(
        language,
        "额度已重置，正在重新采样",
        "Quota reset; collecting new readings",
      )
    : text(
        language,
        "首次采样，正在积累预测数据",
        "Collecting initial readings for a forecast",
      );
  // A partial minute must not claim the minimum has been met.
  const observed =
    elapsed < 60
      ? text(language, "不足1分", "less than 1m")
      : forecast(Math.floor(elapsed / 60) * 60, language);
  const minimum = forecast(required, language);
  return `${reason}\n${text(language, `已采样 ${observed}，预测至少需要${minimum}的有效记录`, `Sampled ${observed}; at least ${minimum} of readings needed`)}`;
}
