import type { QuotaMeasure } from "@agent-hud/protocol";
import type { Language } from "@agent-hud/ui/format";
import { moneyAmount } from "@agent-hud/ui/format";

/** Points set like a "credits" amount: whole numbers have no fraction, others keep at most two places, with grouping. */
export function pointsAmount(amount: number, language: Language): string {
  return moneyAmount(amount, "credits", language);
}

/** The remaining share keeps the decimals the API sent, rather than rounding to an integer. */
export function remainingPercentLabel(
  percent: number,
  language: Language,
): string {
  const text = new Intl.NumberFormat(language === "zh" ? "zh-CN" : "en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(percent);
  return `${text}%`;
}

/** "12,875 points". */
export function grantPointsLabel(
  points: number,
  language: Language,
): string {
  const amount = pointsAmount(points, language);
  return language === "zh" ? `${amount} 积分` : `${amount} points`;
}

/** "Expires in 18 days"; zero days is "Expires today". */
export function grantExpiryLabel(
  days: number,
  language: Language,
): string {
  if (days <= 0) return language === "zh" ? "今天到期" : "Expires today";
  return language === "zh" ? `${days} 天后到期` : `Expires in ${days} days`;
}

/** The share drawn on the panel and the top ring: the first activity that has a percent, clamped to 0–100. */
export function leadingPointsPercent(
  groups: readonly { percent: number | null }[] | undefined,
): number | null {
  const percent = groups?.find(
    (group) => group.percent != null && Number.isFinite(group.percent),
  )?.percent;
  return percent == null ? null : Math.max(0, Math.min(100, percent));
}

/** "12/d". */
export function dailyPaceLabel(perDay: number, language: Language): string {
  const amount = pointsAmount(perDay, language);
  return language === "zh" ? `${amount}/天` : `${amount}/d`;
}

/** Points already spent out of the quota. */
export function pointsSpent(
  total: number | null,
  remaining: number | null,
): number | null {
  if (total == null || remaining == null) return null;
  return Math.max(0, Math.round((total - remaining) * 100) / 100);
}

/** The used share is the complement of the remaining share. */
export function usedPointsPercent(remaining: number): number {
  const used = Math.max(0, Math.min(100, 100 - remaining));
  return Math.round(used * 100) / 100;
}

/** Of the five quota measures, the points detail can switch only remaining share and points spent. */
export function pointsMeasureAvailability(
  language: Language,
): Record<QuotaMeasure, { selectable: boolean; hint: string }> {
  const say = (zh: string, en: string) => (language === "zh" ? zh : en);
  return {
    quota: {
      selectable: true,
      hint: say(
        "积分剩余占总额的比例",
        "Remaining points as a share of the total",
      ),
    },
    consumed: {
      selectable: true,
      hint: say(
        "已用积分，以及近 7 天的日均消耗",
        "Points already spent, and the average per day over the last 7 days",
      ),
    },
    burn: {
      selectable: false,
      hint: say(
        "积分没有按时间采样，算不出消耗速度",
        "Points are not sampled over time, so there is no burn rate",
      ),
    },
    tokens: {
      selectable: false,
      hint: say(
        "积分明细里没有回复速度",
        "The points detail has no reply rate",
      ),
    },
    cost: {
      selectable: false,
      hint: say(
        "积分不按模型标价估算花费",
        "Points are not priced as model list cost",
      ),
    },
  };
}
